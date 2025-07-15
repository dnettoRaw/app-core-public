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
        if max_entries == 0 || ttl_ms == 0 || cleanup_interval_ms == 0 {
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
        Self {
            config,
            state: Mutex::new(ReplayState::default()),
        }
    }

    fn cleanup_locked(&self, state: &mut ReplayState, now_ms: u64) -> usize {
        let before = state.entries.len();
        state.entries.retain(|_, expiry| *expiry > now_ms);
        state
            .lru
            .retain(|identity| state.entries.contains_key(identity));
        let removed = before.saturating_sub(state.entries.len());
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
            state.lru.remove(position);
        }
        state.lru.push_back(identity.to_string());
    }
}

impl PeerNonceStore for BoundedReplayStore {
    fn check_and_record(
        &self,
        nonce: &str,
        expires_at_ms: u64,
        now_ms: u64,
    ) -> Result<(), PeerRpcError> {
        if nonce.is_empty() || expires_at_ms <= now_ms {
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
        let ttl_expiry = now_ms.saturating_add(self.config.ttl_ms);
        let effective_expiry = expires_at_ms.min(ttl_expiry);
        state.entries.insert(nonce.to_string(), effective_expiry);
        state.lru.push_back(nonce.to_string());
        state.metrics.accepted = state.metrics.accepted.saturating_add(1);
        state.metrics.entries = state.entries.len();
        Ok(())
    }
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
        assert_eq!(
            store.check_and_record("nonce-a", 100, 3),
            Err(PeerRpcError::NonceReplay)
        );

        let state = store.state.lock().unwrap();
        assert_eq!(
            state.lru.iter().map(String::as_str).collect::<Vec<_>>(),
            vec!["nonce-b", "nonce-a"]
        );
    }
}
