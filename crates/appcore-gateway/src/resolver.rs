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
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::time::Duration;

/// Stable V1 strategy used to resolve one registered worker.
///
/// This exhaustive enum is frozen with the `FirstAvailable` contract. Use
/// [`WorkerSelectionPolicy`] for opt-in health and admission-aware selection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SelectionPolicy {
    /// Picks the first candidate in stable worker-identity order.
    #[default]
    FirstAvailable,
}

/// Opt-in strategy used to choose one eligible live worker.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WorkerSelectionPolicy {
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
    policy: WorkerSelectionPolicy,
    cursor: Arc<AtomicU64>,
}

impl Default for CapabilityResolver {
    fn default() -> Self {
        Self {
            policy: WorkerSelectionPolicy::FirstAvailable,
            cursor: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl CapabilityResolver {
    /// Creates a resolver with the compatible `FirstAvailable` policy.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a resolver using one explicit live-worker selection policy.
    pub fn with_policy(policy: WorkerSelectionPolicy) -> Self {
        Self {
            policy,
            cursor: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Returns the configured selection policy.
    pub fn policy(&self) -> WorkerSelectionPolicy {
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
        let registered = registry.resolve(capability)?;
        match self.policy {
            WorkerSelectionPolicy::FirstAvailable
            | WorkerSelectionPolicy::LeastInflight
            | WorkerSelectionPolicy::HealthWeighted => registered
                .iter()
                .min_by(|left, right| compare_worker_keys(left, right))
                .cloned(),
            WorkerSelectionPolicy::RoundRobin => {
                let mut candidates = registered.iter().collect::<Vec<_>>();
                candidates.sort_by(|left, right| compare_worker_keys(left, right));
                if candidates.is_empty() {
                    return None;
                }
                Some((*select_cursor(&self.cursor, &candidates)).clone())
            }
            WorkerSelectionPolicy::Affinity => None,
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
        let candidates = healthy_candidates(registered, tenant, input);
        let selected = match self.policy {
            WorkerSelectionPolicy::FirstAvailable => {
                select_best(candidates, input.max_inflight, |candidate, current| {
                    compare_worker_keys(candidate.key, current.key).is_lt()
                })?
            }
            WorkerSelectionPolicy::LeastInflight => {
                select_best(candidates, input.max_inflight, |candidate, current| {
                    compare_candidate_load(candidate, current).is_lt()
                })?
            }
            WorkerSelectionPolicy::Affinity => {
                select_best(candidates, input.max_inflight, |candidate, current| {
                    compare_candidate_affinity(candidate, current, capability, tenant, input)
                        .is_gt()
                })?
            }
            WorkerSelectionPolicy::RoundRobin => self.choose_buffered(
                candidates,
                input.max_inflight,
                BufferedSelectionPolicy::RoundRobin,
            )?,
            WorkerSelectionPolicy::HealthWeighted => self.choose_buffered(
                candidates,
                input.max_inflight,
                BufferedSelectionPolicy::HealthWeighted,
            )?,
        };
        Ok(selected.key.clone())
    }

    fn choose_buffered<'a>(
        &self,
        candidates: impl Iterator<Item = Candidate<'a>>,
        max_inflight: u64,
        policy: BufferedSelectionPolicy,
    ) -> Result<Candidate<'a>, WorkerSelectionError> {
        let mut saw_healthy = false;
        let mut eligible = Vec::with_capacity(candidates.size_hint().1.unwrap_or(0));
        for candidate in candidates {
            saw_healthy = true;
            if candidate.is_eligible(max_inflight) {
                eligible.push(candidate);
            }
        }
        if eligible.is_empty() {
            return Err(empty_selection_error(saw_healthy));
        }
        eligible.sort_by(|left, right| compare_worker_keys(left.key, right.key));
        Ok(match policy {
            BufferedSelectionPolicy::RoundRobin => *select_cursor(&self.cursor, &eligible),
            BufferedSelectionPolicy::HealthWeighted => {
                *select_health_weighted(&self.cursor, &eligible)
            }
        })
    }
}

fn healthy_candidates<'a>(
    registered: &'a HashSet<WorkerConnectionKey>,
    tenant: &'a TenantState,
    input: WorkerSelectionInput<'_>,
) -> impl Iterator<Item = Candidate<'a>> + 'a {
    let now_ms = input.now_ms;
    let heartbeat_timeout = input.heartbeat_timeout;
    registered.iter().filter_map(move |key| {
        if key.tenant_id != tenant.tenant_id {
            return None;
        }
        let worker = tenant.get_worker(&key.installation_id, &key.core_id)?;
        worker
            .is_open_and_healthy(now_ms, heartbeat_timeout)
            .then(|| Candidate::from_worker(worker, now_ms, heartbeat_timeout))
    })
}

fn select_best<'a>(
    candidates: impl Iterator<Item = Candidate<'a>>,
    max_inflight: u64,
    is_better: impl Fn(&Candidate<'a>, &Candidate<'a>) -> bool,
) -> Result<Candidate<'a>, WorkerSelectionError> {
    let mut saw_healthy = false;
    let mut selected = None;
    for candidate in candidates {
        saw_healthy = true;
        if !candidate.is_eligible(max_inflight) {
            continue;
        }
        if selected
            .as_ref()
            .is_none_or(|current| is_better(&candidate, current))
        {
            selected = Some(candidate);
        }
    }
    selected.ok_or_else(|| empty_selection_error(saw_healthy))
}

fn compare_candidate_load(left: &Candidate<'_>, right: &Candidate<'_>) -> Ordering {
    left.inflight
        .cmp(&right.inflight)
        .then_with(|| left.queue_depth.cmp(&right.queue_depth))
        .then_with(|| compare_worker_keys(left.key, right.key))
}

fn compare_candidate_affinity(
    left: &Candidate<'_>,
    right: &Candidate<'_>,
    capability: &CapabilityName,
    tenant: &TenantState,
    input: WorkerSelectionInput<'_>,
) -> Ordering {
    affinity_score(
        tenant.tenant_id.as_str(),
        capability.as_str(),
        input.affinity_key.unwrap_or_default(),
        left.key,
    )
    .cmp(&affinity_score(
        tenant.tenant_id.as_str(),
        capability.as_str(),
        input.affinity_key.unwrap_or_default(),
        right.key,
    ))
    .then_with(|| compare_worker_keys(right.key, left.key))
}

fn empty_selection_error(saw_healthy: bool) -> WorkerSelectionError {
    if saw_healthy {
        WorkerSelectionError::AtCapacity
    } else {
        WorkerSelectionError::NoHealthyWorker
    }
}

#[derive(Clone, Copy)]
enum BufferedSelectionPolicy {
    RoundRobin,
    HealthWeighted,
}

#[derive(Debug, Clone, Copy)]
struct Candidate<'a> {
    key: &'a WorkerConnectionKey,
    inflight: u64,
    queue_depth: usize,
    queue_remaining: usize,
    health_weight: u64,
}

impl<'a> Candidate<'a> {
    fn from_worker(
        worker: &'a crate::WorkerConnection,
        now_ms: u64,
        heartbeat_timeout: Duration,
    ) -> Self {
        let timeout_ms = duration_ms(heartbeat_timeout);
        let age_ms = now_ms.saturating_sub(worker.last_heartbeat());
        let remaining_ms = timeout_ms.saturating_sub(age_ms);
        let health_weight = 1_u64.saturating_add(
            remaining_ms
                .saturating_mul(15)
                .checked_div(timeout_ms)
                .unwrap_or(0),
        );
        Self {
            key: &worker.key,
            inflight: worker.inflight(),
            queue_depth: worker.outbound_queue_depth(),
            queue_remaining: worker.outbound_queue_remaining(),
            health_weight,
        }
    }

    fn is_eligible(self, max_inflight: u64) -> bool {
        self.inflight < max_inflight && self.queue_remaining > 0
    }
}

fn validate_input(
    policy: WorkerSelectionPolicy,
    input: WorkerSelectionInput<'_>,
) -> Result<(), WorkerSelectionError> {
    if input.heartbeat_timeout.is_zero()
        || input.max_inflight == 0
        || input.max_inflight > MAX_GATEWAY_WORKER_INFLIGHT
    {
        return Err(WorkerSelectionError::InvalidLimits);
    }
    if policy == WorkerSelectionPolicy::Affinity {
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

fn select_health_weighted<'a, 'worker>(
    cursor: &AtomicU64,
    candidates: &'a [Candidate<'worker>],
) -> &'a Candidate<'worker> {
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
