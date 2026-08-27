// =============================================================================
//        #######
//     ###       ###     F: resolver.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/26 08:53:09 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Deterministic tenant-local capability worker selection.

use crate::config::{MAX_GATEWAY_AFFINITY_KEY_BYTES, MAX_GATEWAY_WORKER_INFLIGHT};
use crate::connection::WorkerConnectionKey;
use crate::registry::CapabilityRegistry;
use crate::tenant::TenantState;
use appcore_types::CapabilityName;
use std::cmp::Ordering;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::time::Duration;

/// Strategy used to choose one eligible worker from a tenant capability set.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SelectionPolicy {
    /// Picks the first candidate in stable worker-identity order.
    #[default]
    FirstAvailable,
    /// Advances a bounded tenant-local cursor over stable candidate order.
    RoundRobin,
    /// Chooses the smallest admitted-route count and queue depth.
    LeastInflight,
    /// Distributes using fixed heartbeat-freshness weights.
    HealthWeighted,
    /// Uses stateless tenant-local rendezvous hashing.
    Affinity,
}

/// Bounded inputs used by health, admission and affinity-aware selection.
#[derive(Debug, Clone, Copy)]
pub struct WorkerSelectionInput<'a> {
    now_ms: u64,
    heartbeat_timeout: Duration,
    max_inflight: u64,
    affinity_key: Option<&'a str>,
}

impl<'a> WorkerSelectionInput<'a> {
    /// Creates selection input with the fixed Gateway per-worker route limit.
    pub fn new(now_ms: u64, heartbeat_timeout: Duration) -> Self {
        Self {
            now_ms,
            heartbeat_timeout,
            max_inflight: MAX_GATEWAY_WORKER_INFLIGHT,
            affinity_key: None,
        }
    }

    /// Applies a smaller positive per-worker limit for this selection.
    pub fn with_max_inflight(mut self, max_inflight: u64) -> Self {
        self.max_inflight = max_inflight;
        self
    }

    /// Supplies a bounded request affinity key. The resolver never stores it.
    pub fn with_affinity(mut self, affinity_key: &'a str) -> Self {
        self.affinity_key = Some(affinity_key);
        self
    }
}

/// Controlled reason why no worker could be selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum WorkerSelectionError {
    /// No registered worker advertises the requested capability.
    #[error("capability has no registered worker")]
    CapabilityUnavailable,
    /// Registered workers are disconnected or outside the health window.
    #[error("capability has no healthy worker")]
    NoHealthyWorker,
    /// Every healthy worker reached its route or outbound-queue limit.
    #[error("all healthy workers are at capacity")]
    AtCapacity,
    /// Affinity policy requires a non-empty bounded key.
    #[error("affinity policy requires a valid bounded key")]
    InvalidAffinity,
    /// Health and in-flight bounds must be positive and within Runtime limits.
    #[error("worker selection limits are invalid")]
    InvalidLimits,
}

/// Resolves worker targets for capability requests within one tenant partition.
#[derive(Debug, Clone)]
pub struct CapabilityResolver {
    policy: SelectionPolicy,
    cursor: Arc<AtomicU64>,
}

impl Default for CapabilityResolver {
    fn default() -> Self {
        Self {
            policy: SelectionPolicy::FirstAvailable,
            cursor: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl CapabilityResolver {
    /// Creates a resolver with the compatible `FirstAvailable` policy.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a resolver using one explicit 2.0 selection policy.
    pub fn with_policy(policy: SelectionPolicy) -> Self {
        Self {
            policy,
            cursor: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Returns the configured selection policy.
    pub fn policy(&self) -> SelectionPolicy {
        self.policy
    }

    /// Resolves from registry data alone.
    ///
    /// This compatible planner cannot evaluate live health or admission. Use
    /// [`Self::select`] before dispatch when those guarantees are required.
    pub fn resolve(
        &self,
        capability: &CapabilityName,
        registry: &CapabilityRegistry,
    ) -> Option<WorkerConnectionKey> {
        let mut candidates = registry.resolve(capability)?.iter().collect::<Vec<_>>();
        candidates.sort_by(|left, right| compare_worker_keys(left, right));
        if candidates.is_empty() {
            return None;
        }
        match self.policy {
            SelectionPolicy::FirstAvailable
            | SelectionPolicy::LeastInflight
            | SelectionPolicy::HealthWeighted => candidates.first().map(|key| (*key).clone()),
            SelectionPolicy::RoundRobin => {
                Some((*select_cursor(&self.cursor, &candidates)).clone())
            }
            SelectionPolicy::Affinity => None,
        }
    }

    /// Selects one live, healthy and admitted worker from a tenant partition.
    pub fn select(
        &self,
        capability: &CapabilityName,
        tenant: &TenantState,
        input: WorkerSelectionInput<'_>,
    ) -> Result<WorkerConnectionKey, WorkerSelectionError> {
        validate_input(self.policy, input)?;
        let registered = tenant
            .registry
            .resolve(capability)
            .ok_or(WorkerSelectionError::CapabilityUnavailable)?;
        let mut healthy = Vec::with_capacity(registered.len());
        for key in registered {
            if key.tenant_id != tenant.tenant_id {
                continue;
            }
            let Some(worker) = tenant.get_worker(&key.installation_id, &key.core_id) else {
                continue;
            };
            if worker.is_open_and_healthy(input.now_ms, input.heartbeat_timeout) {
                healthy.push(Candidate::from_worker(worker, input));
            }
        }
        if healthy.is_empty() {
            return Err(WorkerSelectionError::NoHealthyWorker);
        }
        healthy.sort_by(|left, right| compare_worker_keys(&left.key, &right.key));
        let eligible = healthy
            .into_iter()
            .filter(|candidate| {
                candidate.inflight < input.max_inflight && candidate.queue_remaining > 0
            })
            .collect::<Vec<_>>();
        if eligible.is_empty() {
            return Err(WorkerSelectionError::AtCapacity);
        }
        Ok(self
            .choose(&eligible, capability, tenant, input)
            .key
            .clone())
    }

    fn choose<'a>(
        &self,
        candidates: &'a [Candidate],
        capability: &CapabilityName,
        tenant: &TenantState,
        input: WorkerSelectionInput<'_>,
    ) -> &'a Candidate {
        match self.policy {
            SelectionPolicy::FirstAvailable => &candidates[0],
            SelectionPolicy::RoundRobin => select_cursor(&self.cursor, candidates),
            SelectionPolicy::LeastInflight => candidates
                .iter()
                .min_by(|left, right| {
                    left.inflight
                        .cmp(&right.inflight)
                        .then_with(|| left.queue_depth.cmp(&right.queue_depth))
                        .then_with(|| compare_worker_keys(&left.key, &right.key))
                })
                .unwrap_or(&candidates[0]),
            SelectionPolicy::HealthWeighted => select_health_weighted(&self.cursor, candidates),
            SelectionPolicy::Affinity => select_affinity(
                candidates,
                tenant.tenant_id.as_str(),
                capability.as_str(),
                input.affinity_key.unwrap_or_default(),
            ),
        }
    }
}

#[derive(Debug)]
struct Candidate {
    key: WorkerConnectionKey,
    inflight: u64,
    queue_depth: usize,
    queue_remaining: usize,
    health_weight: u64,
}

impl Candidate {
    fn from_worker(worker: &crate::WorkerConnection, input: WorkerSelectionInput<'_>) -> Self {
        let timeout_ms = duration_ms(input.heartbeat_timeout);
        let age_ms = input.now_ms.saturating_sub(worker.last_heartbeat());
        let remaining_ms = timeout_ms.saturating_sub(age_ms);
        let health_weight = 1_u64.saturating_add(
            remaining_ms
                .saturating_mul(15)
                .checked_div(timeout_ms)
                .unwrap_or(0),
        );
        Self {
            key: worker.key.clone(),
            inflight: worker.inflight(),
            queue_depth: worker.outbound_queue_depth(),
            queue_remaining: worker.outbound_queue_remaining(),
            health_weight,
        }
    }
}

fn validate_input(
    policy: SelectionPolicy,
    input: WorkerSelectionInput<'_>,
) -> Result<(), WorkerSelectionError> {
    if input.heartbeat_timeout.is_zero()
        || input.max_inflight == 0
        || input.max_inflight > MAX_GATEWAY_WORKER_INFLIGHT
    {
        return Err(WorkerSelectionError::InvalidLimits);
    }
    if policy == SelectionPolicy::Affinity {
        let affinity = input
            .affinity_key
            .filter(|value| !value.is_empty())
            .filter(|value| value.len() <= MAX_GATEWAY_AFFINITY_KEY_BYTES)
            .filter(|value| !value.chars().any(char::is_control));
        if affinity.is_none() {
            return Err(WorkerSelectionError::InvalidAffinity);
        }
    }
    Ok(())
}

fn select_cursor<'a, T>(cursor: &AtomicU64, candidates: &'a [T]) -> &'a T {
    let ticket = cursor.fetch_add(1, AtomicOrdering::Relaxed);
    let index = usize::try_from(ticket).unwrap_or(usize::MAX) % candidates.len();
    &candidates[index]
}

fn select_health_weighted<'a>(cursor: &AtomicU64, candidates: &'a [Candidate]) -> &'a Candidate {
    let total = candidates.iter().fold(0_u64, |sum, candidate| {
        sum.saturating_add(candidate.health_weight)
    });
    let mut slot = cursor.fetch_add(1, AtomicOrdering::Relaxed) % total.max(1);
    for candidate in candidates {
        if slot < candidate.health_weight {
            return candidate;
        }
        slot = slot.saturating_sub(candidate.health_weight);
    }
    &candidates[0]
}

fn select_affinity<'a>(
    candidates: &'a [Candidate],
    tenant: &str,
    capability: &str,
    affinity: &str,
) -> &'a Candidate {
    candidates
        .iter()
        .max_by(|left, right| {
            affinity_score(tenant, capability, affinity, &left.key)
                .cmp(&affinity_score(tenant, capability, affinity, &right.key))
                .then_with(|| compare_worker_keys(&right.key, &left.key))
        })
        .unwrap_or(&candidates[0])
}

fn affinity_score(
    tenant: &str,
    capability: &str,
    affinity: &str,
    worker: &WorkerConnectionKey,
) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for value in [
        tenant,
        capability,
        affinity,
        worker.tenant_id.as_str(),
        worker.installation_id.as_str(),
        worker.core_id.as_str(),
    ] {
        for byte in (value.len() as u64)
            .to_le_bytes()
            .iter()
            .chain(value.as_bytes())
        {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

fn compare_worker_keys(left: &WorkerConnectionKey, right: &WorkerConnectionKey) -> Ordering {
    left.tenant_id
        .as_str()
        .cmp(right.tenant_id.as_str())
        .then_with(|| {
            left.installation_id
                .as_str()
                .cmp(right.installation_id.as_str())
        })
        .then_with(|| left.core_id.as_str().cmp(right.core_id.as_str()))
}

fn duration_ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "resolver_tests.rs"]
mod tests;
