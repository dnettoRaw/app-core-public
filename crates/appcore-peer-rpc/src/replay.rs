// =============================================================================
//        #######
//     ###       ###     F: replay.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/24 11:51:10 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 11:51:10 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Bounded replay-store policy shared by authenticated Runtime ingress.

use crate::{PeerNonceStore, PeerRpcError};
use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

/// Absolute aggregate retained-byte ceiling accepted by the process-local store.
pub const MAX_REPLAY_STORE_BYTES: usize = 32 * 1024 * 1024;
const REPLAY_ENTRY_FIXED_BYTES: usize = std::mem::size_of::<(String, u64)>()
    + std::mem::size_of::<String>()
    + std::mem::size_of::<usize>() * 4;

/// Capacity, TTL, and cleanup policy for a replay store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayStoreConfig {
    /// Maximum number of live replay identities.
    pub max_entries: usize,
    /// Maximum time a caller-provided expiry may retain an identity.
    pub ttl_ms: u64,
    /// Maximum interval between full expired-entry cleanup passes.
    pub cleanup_interval_ms: u64,
}

impl ReplayStoreConfig {
    /// Validates and creates a replay-store policy.
    pub fn new(
        max_entries: usize,
        ttl_ms: u64,
        cleanup_interval_ms: u64,
    ) -> Result<Self, PeerRpcError> {
        if max_entries == 0
            || max_entries > super::MAX_NONCE_CACHE_ENTRIES
            || ttl_ms == 0
            || cleanup_interval_ms == 0
        {
            return Err(PeerRpcError::InvalidEnvelope(
                "replay_store_policy_invalid".to_string(),
            ));
        }
        Ok(Self {
            max_entries,
            ttl_ms,
            cleanup_interval_ms,
        })
    }
}

impl Default for ReplayStoreConfig {
    fn default() -> Self {
        Self {
            max_entries: super::MAX_NONCE_CACHE_ENTRIES,
            ttl_ms: 60_000,
            cleanup_interval_ms: 1_000,
        }
    }
}

/// Point-in-time replay-store metrics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReplayStoreMetrics {
    /// Current number of live entries.
    pub entries: usize,
    /// Identities accepted since creation.
    pub accepted: u64,
    /// Live replay attempts rejected since creation.
    pub replays: u64,
    /// Expired identities removed since creation.
    pub expired: u64,
    /// Requests rejected because all bounded entries were live.
    pub capacity_rejections: u64,
    /// Full cleanup passes executed since creation.
    pub cleanups: u64,
}

/// Point-in-time retained-memory metrics for a bounded replay store.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReplayStoreMemoryMetrics {
    /// Estimated bytes retained by live replay identities and LRU keys.
    pub used_bytes: usize,
    /// Highest estimated retained byte count observed since creation.
    pub peak_bytes: usize,
    /// Configured aggregate retained-byte ceiling.
    pub max_bytes: usize,
    /// Requests rejected specifically because the byte ceiling was full.
    pub byte_rejections: u64,
}

/// Replay protection with explicit cleanup and observable bounds.
pub trait ReplayStore: PeerNonceStore {
    /// Removes entries whose effective TTL has elapsed.
    fn cleanup(&self, now_ms: u64) -> Result<usize, PeerRpcError>;
    /// Returns bounded non-sensitive metrics.
    fn metrics(&self) -> ReplayStoreMetrics;
}

#[derive(Debug, Default)]
struct ReplayState {
    entries: BTreeMap<String, u64>,
    lru: VecDeque<String>,
    last_cleanup_ms: u64,
    metrics: ReplayStoreMetrics,
    memory: ReplayStoreMemoryMetrics,
}

/// Process-local TTL replay store with LRU ordering and fail-closed capacity.
///
/// Live entries are never evicted merely to make space because doing so would
/// reopen their replay window. LRU ordering is used to remove expired entries
/// deterministically; a full live set rejects new requests.
#[derive(Debug)]
pub struct BoundedReplayStore {
    config: ReplayStoreConfig,
    state: Mutex<ReplayState>,
}

impl BoundedReplayStore {
    /// Creates an empty bounded replay store.
    pub fn new(config: ReplayStoreConfig) -> Self {
        let max_bytes = default_max_bytes(config.max_entries);
        Self {
            config,
            state: Mutex::new(ReplayState {
                memory: ReplayStoreMemoryMetrics {
                    max_bytes,
                    ..ReplayStoreMemoryMetrics::default()
                },
                ..ReplayState::default()
            }),
        }
    }

    /// Creates a replay store with a tighter aggregate retained-byte ceiling.
    pub fn with_max_bytes(
        config: ReplayStoreConfig,
        max_bytes: usize,
    ) -> Result<Self, PeerRpcError> {
        let default_max = default_max_bytes(config.max_entries);
        if max_bytes == 0 || max_bytes > default_max {
            return Err(PeerRpcError::InvalidEnvelope(
                "replay_store_policy_invalid".to_string(),
            ));
        }
        Ok(Self {
            config,
            state: Mutex::new(ReplayState {
                memory: ReplayStoreMemoryMetrics {
                    max_bytes,
                    ..ReplayStoreMemoryMetrics::default()
                },
                ..ReplayState::default()
            }),
        })
    }

    /// Returns bounded non-sensitive retained-memory metrics.
    pub fn memory_metrics(&self) -> ReplayStoreMemoryMetrics {
        self.state
            .lock()
            .map(|state| state.memory)
            .unwrap_or_default()
    }

    fn cleanup_locked(&self, state: &mut ReplayState, now_ms: u64) -> usize {
        let mut removed = 0usize;
        let mut removed_bytes = 0usize;
        let entries = &mut state.entries;
        state.lru.retain(|identity| {
            let live = entries.get(identity).is_some_and(|expiry| *expiry > now_ms);
            if live {
                return true;
            }
            if entries.remove(identity).is_some() {
                removed = removed.saturating_add(1);
                removed_bytes = removed_bytes.saturating_add(replay_entry_retained_bytes(identity));
            }
            false
        });
        state.memory.used_bytes = state.memory.used_bytes.saturating_sub(removed_bytes);
        state.last_cleanup_ms = now_ms;
        state.metrics.expired = state.metrics.expired.saturating_add(removed as u64);
        state.metrics.cleanups = state.metrics.cleanups.saturating_add(1);
        state.metrics.entries = state.entries.len();
        removed
    }

    fn cleanup_if_due(&self, state: &mut ReplayState, now_ms: u64) {
        if now_ms.saturating_sub(state.last_cleanup_ms) >= self.config.cleanup_interval_ms {
            self.cleanup_locked(state, now_ms);
        }
    }

    fn touch_lru(state: &mut ReplayState, identity: &str) {
        if let Some(position) = state.lru.iter().position(|entry| entry == identity) {
            if let Some(identity) = state.lru.remove(position) {
                state.lru.push_back(identity);
            }
        }
    }
}

impl PeerNonceStore for BoundedReplayStore {
    fn check_and_record(
        &self,
        nonce: &str,
        expires_at_ms: u64,
        now_ms: u64,
    ) -> Result<(), PeerRpcError> {
        crate::nonce::validate_nonce(nonce)?;
        if expires_at_ms <= now_ms {
            return Err(PeerRpcError::InvalidEnvelope(
                "replay_identity_invalid".to_string(),
            ));
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| PeerRpcError::InvalidEnvelope("replay_store_poisoned".to_string()))?;
        self.cleanup_if_due(&mut state, now_ms);
        if state.entries.contains_key(nonce) {
            Self::touch_lru(&mut state, nonce);
            state.metrics.replays = state.metrics.replays.saturating_add(1);
            return Err(PeerRpcError::NonceReplay);
        }
        if state.entries.len() >= self.config.max_entries {
            self.cleanup_locked(&mut state, now_ms);
            if state.entries.len() >= self.config.max_entries {
                state.metrics.capacity_rejections =
                    state.metrics.capacity_rejections.saturating_add(1);
                return Err(PeerRpcError::NonceCacheFull);
            }
        }
        let retained_bytes = replay_entry_retained_bytes(nonce);
        if state.memory.used_bytes.saturating_add(retained_bytes) > state.memory.max_bytes {
            state.metrics.capacity_rejections = state.metrics.capacity_rejections.saturating_add(1);
            state.memory.byte_rejections = state.memory.byte_rejections.saturating_add(1);
            return Err(PeerRpcError::NonceCacheFull);
        }
        let ttl_expiry = now_ms.saturating_add(self.config.ttl_ms);
        let effective_expiry = expires_at_ms.min(ttl_expiry);
        state.entries.insert(nonce.to_string(), effective_expiry);
        state.lru.push_back(nonce.to_string());
        state.memory.used_bytes = state.memory.used_bytes.saturating_add(retained_bytes);
        state.memory.peak_bytes = state.memory.peak_bytes.max(state.memory.used_bytes);
        state.metrics.accepted = state.metrics.accepted.saturating_add(1);
        state.metrics.entries = state.entries.len();
        Ok(())
    }
}

fn default_max_bytes(max_entries: usize) -> usize {
    max_entries
        .saturating_mul(
            REPLAY_ENTRY_FIXED_BYTES
                .saturating_add(crate::nonce::MAX_NONCE_BYTES.saturating_mul(2)),
        )
        .clamp(1, MAX_REPLAY_STORE_BYTES)
}

fn replay_entry_retained_bytes(identity: &str) -> usize {
    REPLAY_ENTRY_FIXED_BYTES.saturating_add(identity.len().saturating_mul(2))
}

impl ReplayStore for BoundedReplayStore {
    fn cleanup(&self, now_ms: u64) -> Result<usize, PeerRpcError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| PeerRpcError::InvalidEnvelope("replay_store_poisoned".to_string()))?;
        Ok(self.cleanup_locked(&mut state, now_ms))
    }

    fn metrics(&self) -> ReplayStoreMetrics {
        self.state
            .lock()
            .map(|state| state.metrics)
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(max_entries: usize) -> BoundedReplayStore {
        BoundedReplayStore::new(ReplayStoreConfig::new(max_entries, 100, 10).unwrap())
    }

    #[test]
    fn rejects_live_replay_and_reports_metrics() {
        let store = store(2);
        assert!(store.check_and_record("nonce-a", 100, 1).is_ok());
        assert_eq!(
            store.check_and_record("nonce-a", 100, 2),
            Err(PeerRpcError::NonceReplay)
        );
        assert_eq!(
            store.metrics(),
            ReplayStoreMetrics {
                entries: 1,
                accepted: 1,
                replays: 1,
                ..ReplayStoreMetrics::default()
            }
        );
    }

    #[test]
    fn policy_and_identity_validation_enforce_the_shared_nonce_bounds() {
        assert!(
            ReplayStoreConfig::new(super::super::MAX_NONCE_CACHE_ENTRIES + 1, 100, 10).is_err()
        );
        let store = store(1);
        let oversized = "n".repeat(crate::nonce::MAX_NONCE_BYTES + 1);
        assert!(matches!(
            store.check_and_record(&oversized, 100, 1),
            Err(PeerRpcError::InvalidEnvelope(_))
        ));
        assert_eq!(store.metrics().entries, 0);
        assert_eq!(store.memory_metrics().used_bytes, 0);
    }

    #[test]
    fn byte_budget_fails_closed_and_cleanup_releases_accounted_memory() {
        let config = ReplayStoreConfig::new(2, 100, 10).unwrap();
        let one_entry = replay_entry_retained_bytes("nonce-a");
        let store = BoundedReplayStore::with_max_bytes(config, one_entry).unwrap();
        assert!(store.check_and_record("nonce-a", 100, 1).is_ok());
        assert_eq!(
            store.check_and_record("nonce-b", 100, 2),
            Err(PeerRpcError::NonceCacheFull)
        );
        assert_eq!(
            store.memory_metrics(),
            ReplayStoreMemoryMetrics {
                used_bytes: one_entry,
                peak_bytes: one_entry,
                max_bytes: one_entry,
                byte_rejections: 1,
            }
        );
        assert_eq!(store.metrics().capacity_rejections, 1);

        assert_eq!(store.cleanup(101).unwrap(), 1);
        let memory = store.memory_metrics();
        assert_eq!(memory.used_bytes, 0);
        assert_eq!(memory.peak_bytes, one_entry);
    }

    #[test]
    fn concurrent_replay_admission_accepts_exactly_one_request() {
        let store = std::sync::Arc::new(store(8));
        let workers = (0..8)
            .map(|_| {
                let store = store.clone();
                std::thread::spawn(move || store.check_and_record("shared-nonce", 100, 1))
            })
            .collect::<Vec<_>>();
        let outcomes = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            outcomes
                .iter()
                .filter(|result| matches!(result, Err(PeerRpcError::NonceReplay)))
                .count(),
            7
        );
        assert_eq!(store.metrics().entries, 1);
        let memory = store.memory_metrics();
        assert!(memory.used_bytes > 0);
        assert!(memory.used_bytes <= memory.max_bytes);
    }

    #[test]
    fn expires_entries_and_applies_ttl_cap() {
        let store = store(1);
        assert!(store.check_and_record("nonce-a", 10_000, 1).is_ok());
        assert_eq!(
            store.check_and_record("nonce-b", 10_000, 2),
            Err(PeerRpcError::NonceCacheFull)
        );
        assert!(store.check_and_record("nonce-b", 10_000, 102).is_ok());
        assert_eq!(store.metrics().entries, 1);
        assert_eq!(store.metrics().expired, 1);
    }

    #[test]
    fn full_live_store_fails_closed_without_lru_eviction() {
        let store = store(1);
        store.check_and_record("nonce-a", 100, 1).unwrap();

        assert_eq!(
            store.check_and_record("nonce-b", 100, 2),
            Err(PeerRpcError::NonceCacheFull)
        );
        assert_eq!(
            store.check_and_record("nonce-a", 100, 3),
            Err(PeerRpcError::NonceReplay)
        );
        assert_eq!(store.metrics().capacity_rejections, 1);
    }

    #[test]
    fn replay_touch_updates_lru_age_without_evicting_live_entries() {
        let store = store(2);
        store.check_and_record("nonce-a", 100, 1).unwrap();
        store.check_and_record("nonce-b", 100, 2).unwrap();
        let memory_before = store.memory_metrics();
        assert_eq!(
            store.check_and_record("nonce-a", 100, 3),
            Err(PeerRpcError::NonceReplay)
        );

        let state = store.state.lock().unwrap();
        assert_eq!(
            state.lru.iter().map(String::as_str).collect::<Vec<_>>(),
            vec!["nonce-b", "nonce-a"]
        );
        drop(state);
        assert_eq!(store.memory_metrics(), memory_before);
    }
}
