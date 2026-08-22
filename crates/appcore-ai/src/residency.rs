// =============================================================================
//        #######
//     ###       ###     F: residency.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::residency_validation::{validate_record, validate_request};
use crate::{
    AiError, AiResourceMode, AiResult, CancellationToken, DeviceId, ModelId, PeerId,
    ResidencyMetrics,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

const MAX_RESIDENT_RECORDS: usize = 4_096;
const MAX_PENDING_RESERVATIONS: usize = 256;

/// Ordered model-residency tier from accelerators through peer storage.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ResidencyTier {
    /// Device-local accelerator memory.
    Vram(DeviceId),
    /// Local system memory.
    Memory,
    /// Validated local persistent cache.
    LocalStorage,
    /// Authenticated peer artifact storage.
    Peer(PeerId),
}

/// Fixed capacity of one available residency tier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TierCapacity {
    /// Available tier.
    pub tier: ResidencyTier,
    /// Physical or policy-capped bytes.
    pub capacity_bytes: u64,
}

/// Bounds applied to residency and speculative prefetch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResidencyConfig {
    /// Maximum non-unrestricted fill in basis points.
    pub max_fill_basis_points: u16,
    /// Maximum simultaneous prefetch reservations.
    pub max_concurrent_prefetch: usize,
    /// Maximum artifact size eligible for prefetch.
    pub max_prefetch_bytes: u64,
    /// Maximum fallback tiers examined for one request.
    pub max_fallback_tiers: usize,
}

impl ResidencyConfig {
    /// Validates bounded configuration.
    pub fn validate(self) -> AiResult<Self> {
        if self.max_fill_basis_points == 0
            || self.max_fill_basis_points > 10_000
            || self.max_concurrent_prefetch == 0
            || self.max_prefetch_bytes == 0
            || self.max_fallback_tiers == 0
            || self.max_fallback_tiers > 16
        {
            return Err(AiError::InvalidInput("residency configuration"));
        }
        Ok(self)
    }
}

impl Default for ResidencyConfig {
    fn default() -> Self {
        Self {
            max_fill_basis_points: 8_500,
            max_concurrent_prefetch: 2,
            max_prefetch_bytes: 2 * 1024 * 1024 * 1024,
            max_fallback_tiers: 4,
        }
    }
}

/// Usage metadata used by the initial deterministic LRU policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResidencyRecord {
    /// Logical model.
    pub model: ModelId,
    /// Current tier.
    pub tier: ResidencyTier,
    /// Resident bytes.
    pub size_bytes: u64,
    /// Last observed use from an injected monotonic clock.
    pub last_used_ms: u64,
    /// Saturating use count.
    pub use_count: u64,
    /// Estimated reload time.
    pub load_time_ms: u64,
    /// Capability or quality importance in basis points.
    pub importance_basis_points: u16,
    /// Optional simple next-use prediction.
    pub predicted_next_use_ms: Option<u64>,
}

/// Inputs to one reservation attempt.
#[derive(Clone, Debug)]
pub struct ResidencyRequest {
    /// Logical model.
    pub model: ModelId,
    /// Preferred target tier.
    pub preferred: ResidencyTier,
    /// Ordered degradation tiers when the preferred tier is unavailable.
    pub fallbacks: Vec<ResidencyTier>,
    /// Exact artifact bytes to reserve.
    pub size_bytes: u64,
    /// Estimated load time.
    pub load_time_ms: u64,
    /// Capability or quality importance in basis points.
    pub importance_basis_points: u16,
    /// Optional simple next-use prediction.
    pub predicted_next_use_ms: Option<u64>,
    /// Injected monotonic time.
    pub now_ms: u64,
    /// Active resource mode.
    pub resource_mode: AiResourceMode,
    /// Optional tighter budget supplied by `ResourceGovernor`.
    pub capacity_limit_bytes: Option<u64>,
    /// Whether this is speculative rather than demand-driven loading.
    pub prefetch: bool,
    /// Cooperative cancellation checked before reservation.
    pub cancellation: CancellationToken,
}

/// One eviction selected but not applied until successful commit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResidencyEviction {
    /// Model to remove.
    pub model: ModelId,
    /// Tier to free.
    pub tier: ResidencyTier,
    /// Bytes recovered on commit.
    pub size_bytes: u64,
}

/// Reservation token for a two-phase safe load.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResidencyReservation {
    token: u64,
    /// Model being loaded.
    pub model: ModelId,
    /// Existing source tier when the model is already cached elsewhere.
    pub source: Option<ResidencyTier>,
    /// Selected target tier.
    pub target: ResidencyTier,
    /// Evictions applied only after successful load.
    pub evictions: Vec<ResidencyEviction>,
    /// Reserved artifact bytes.
    pub size_bytes: u64,
}

/// Result of planning model residency.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResidencyDecision {
    /// A hot model is already present; usage was refreshed.
    Reuse {
        /// Reused tier.
        tier: ResidencyTier,
    },
    /// Another request already owns the same load.
    InFlight {
        /// Target currently being loaded.
        tier: ResidencyTier,
    },
    /// Caller may execute the described two-phase load.
    Reserved(ResidencyReservation),
}

#[derive(Clone, Debug)]
pub(crate) struct Pending {
    reservation: ResidencyReservation,
    record: ResidencyRecord,
    prefetch: bool,
}

#[derive(Debug, Default)]
pub(crate) struct State {
    capacities: BTreeMap<ResidencyTier, u64>,
    pub(crate) residents: BTreeMap<(ResidencyTier, ModelId), ResidencyRecord>,
    pub(crate) pending: BTreeMap<u64, Pending>,
    next_token: u64,
    pub(crate) active_prefetch: usize,
    pub(crate) metrics: ResidencyMetrics,
}

/// Thread-safe LRU residency planner with two-phase rollback semantics.
#[derive(Debug)]
pub struct ResidencyPlanner {
    config: ResidencyConfig,
    pub(crate) state: Mutex<State>,
}

impl ResidencyPlanner {
    /// Creates a planner from explicit virtual or physical tier capacities.
    pub fn new(config: ResidencyConfig, capacities: Vec<TierCapacity>) -> AiResult<Self> {
        let config = config.validate()?;
        if capacities.is_empty() || capacities.len() > 32 {
            return Err(AiError::InvalidInput("residency capacities"));
        }
        let mut state = State {
            next_token: 1,
            ..State::default()
        };
        for capacity in capacities {
            if capacity.capacity_bytes == 0
                || state
                    .capacities
                    .insert(capacity.tier, capacity.capacity_bytes)
                    .is_some()
            {
                return Err(AiError::InvalidInput("residency tier capacity"));
            }
        }
        Ok(Self {
            config,
            state: Mutex::new(state),
        })
    }

    /// Registers a verified pre-existing resident model.
    pub fn register(&self, record: ResidencyRecord) -> AiResult<()> {
        validate_record(&record)?;
        let mut state = self.state.lock().map_err(|_| AiError::InternalState)?;
        let capacity = state
            .capacities
            .get(&record.tier)
            .copied()
            .ok_or(AiError::NotFound("residency tier"))?;
        let used = used_bytes(&state, &record.tier);
        if used.saturating_add(record.size_bytes) > capacity {
            return Err(AiError::Capacity("residency tier"));
        }
        if state
            .residents
            .contains_key(&(record.tier.clone(), record.model.clone()))
        {
            return Err(AiError::Conflict("resident model"));
        }
        if state.residents.len() >= MAX_RESIDENT_RECORDS {
            return Err(AiError::Capacity("resident model records"));
        }
        state
            .residents
            .insert((record.tier.clone(), record.model.clone()), record);
        Ok(())
    }

    /// Reserves a reuse, promotion or bounded degradation plan.
    pub fn begin(&self, request: ResidencyRequest) -> AiResult<ResidencyDecision> {
        validate_request(&request, self.config)?;
        if request.cancellation.is_cancelled() {
            return Err(AiError::Cancelled);
        }
        let mut state = self.state.lock().map_err(|_| AiError::InternalState)?;
        if request.prefetch && state.active_prefetch >= self.config.max_concurrent_prefetch {
            return Err(AiError::Capacity("residency prefetch"));
        }
        let mut tiers = Vec::with_capacity(request.fallbacks.len().saturating_add(1));
        tiers.push(request.preferred.clone());
        tiers.extend(request.fallbacks.iter().cloned());
        for tier in tiers {
            if let Some(record) = state
                .residents
                .get_mut(&(tier.clone(), request.model.clone()))
            {
                record.last_used_ms = request.now_ms;
                record.use_count = record.use_count.saturating_add(1);
                record.predicted_next_use_ms = request.predicted_next_use_ms;
                state.metrics.reuses = state.metrics.reuses.saturating_add(1);
                return Ok(ResidencyDecision::Reuse { tier });
            }
            if state.pending.values().any(|pending| {
                pending.reservation.model == request.model && pending.reservation.target == tier
            }) {
                state.metrics.in_flight = state.metrics.in_flight.saturating_add(1);
                return Ok(ResidencyDecision::InFlight { tier });
            }
            if let Some(reservation) = reserve_tier(&mut state, &request, &tier, self.config)? {
                if request.prefetch {
                    state.active_prefetch = state.active_prefetch.saturating_add(1);
                }
                state.metrics.reservations = state.metrics.reservations.saturating_add(1);
                return Ok(ResidencyDecision::Reserved(reservation));
            }
        }
        Err(AiError::Capacity("model residency"))
    }

    /// Commits a successful load or rolls back a failed/cancelled load atomically.
    pub fn finish(
        &self,
        reservation: ResidencyReservation,
        success: bool,
        now_ms: u64,
    ) -> AiResult<()> {
        let mut state = self.state.lock().map_err(|_| AiError::InternalState)?;
        let pending = state
            .pending
            .remove(&reservation.token)
            .ok_or(AiError::NotFound("residency reservation"))?;
        if pending.reservation != reservation {
            return Err(AiError::Conflict("residency reservation"));
        }
        if pending.prefetch {
            state.active_prefetch = state.active_prefetch.saturating_sub(1);
        }
        if !success {
            state.metrics.rollbacks = state.metrics.rollbacks.saturating_add(1);
            return Ok(());
        }
        state.metrics.evictions = state
            .metrics
            .evictions
            .saturating_add(u64::try_from(reservation.evictions.len()).unwrap_or(u64::MAX));
        for eviction in &reservation.evictions {
            state
                .residents
                .remove(&(eviction.tier.clone(), eviction.model.clone()));
        }
        let mut record = pending.record;
        record.last_used_ms = now_ms;
        state
            .residents
            .insert((record.tier.clone(), record.model.clone()), record);
        Ok(())
    }

    /// Returns a stable snapshot for diagnostics and tests.
    pub fn snapshot(&self) -> AiResult<Vec<ResidencyRecord>> {
        Ok(self
            .state
            .lock()
            .map_err(|_| AiError::InternalState)?
            .residents
            .values()
            .cloned()
            .collect())
    }
}

fn reserve_tier(
    state: &mut State,
    request: &ResidencyRequest,
    tier: &ResidencyTier,
    config: ResidencyConfig,
) -> AiResult<Option<ResidencyReservation>> {
    if state.pending.len() >= MAX_PENDING_RESERVATIONS {
        return Err(AiError::Capacity("residency reservations"));
    }
    let Some(capacity) = effective_tier_capacity(state, request, tier, config) else {
        return Ok(None);
    };
    let Some(evictions) = tier_evictions(state, request, tier, capacity) else {
        return Ok(None);
    };
    let source = state
        .residents
        .values()
        .find(|record| record.model == request.model)
        .map(|record| record.tier.clone());
    let token = state.next_token;
    state.next_token = state.next_token.saturating_add(1);
    let reservation = ResidencyReservation {
        token,
        model: request.model.clone(),
        source,
        target: tier.clone(),
        evictions,
        size_bytes: request.size_bytes,
    };
    state.pending.insert(
        token,
        Pending {
            reservation: reservation.clone(),
            record: ResidencyRecord {
                model: request.model.clone(),
                tier: tier.clone(),
                size_bytes: request.size_bytes,
                last_used_ms: request.now_ms,
                use_count: 1,
                load_time_ms: request.load_time_ms,
                importance_basis_points: request.importance_basis_points,
                predicted_next_use_ms: request.predicted_next_use_ms,
            },
            prefetch: request.prefetch,
        },
    );
    Ok(Some(reservation))
}

fn effective_tier_capacity(
    state: &State,
    request: &ResidencyRequest,
    tier: &ResidencyTier,
    config: ResidencyConfig,
) -> Option<u64> {
    let declared = state.capacities.get(tier).copied()?;
    let mut capacity = request
        .capacity_limit_bytes
        .map_or(declared, |limit| declared.min(limit));
    if request.resource_mode != AiResourceMode::Unrestricted {
        capacity = capacity.saturating_mul(u64::from(config.max_fill_basis_points)) / 10_000;
    }
    (request.size_bytes <= capacity).then_some(capacity)
}

fn tier_evictions(
    state: &State,
    request: &ResidencyRequest,
    tier: &ResidencyTier,
    capacity: u64,
) -> Option<Vec<ResidencyEviction>> {
    let pending_bytes = state
        .pending
        .values()
        .filter(|pending| &pending.reservation.target == tier)
        .fold(0u64, |sum, pending| {
            sum.saturating_add(pending.reservation.size_bytes)
        });
    let used = used_bytes(state, tier).saturating_add(pending_bytes);
    let needed = used
        .saturating_add(request.size_bytes)
        .saturating_sub(capacity);
    let reserved_evictions = state
        .pending
        .values()
        .flat_map(|pending| pending.reservation.evictions.iter())
        .map(|eviction| (eviction.tier.clone(), eviction.model.clone()))
        .collect::<BTreeSet<_>>();
    let mut candidates = state
        .residents
        .values()
        .filter(|record| {
            &record.tier == tier
                && record.model != request.model
                && !reserved_evictions.contains(&(record.tier.clone(), record.model.clone()))
        })
        .cloned()
        .collect::<Vec<_>>();
    candidates.sort_by_key(|record| {
        (
            record.importance_basis_points,
            record.use_count,
            record.predicted_next_use_ms.unwrap_or(u64::MAX),
            record.last_used_ms,
            record.load_time_ms,
            record.model.clone(),
        )
    });
    let mut recovered = 0u64;
    let mut evictions = Vec::new();
    for candidate in candidates {
        if recovered >= needed {
            break;
        }
        recovered = recovered.saturating_add(candidate.size_bytes);
        evictions.push(ResidencyEviction {
            model: candidate.model,
            tier: candidate.tier,
            size_bytes: candidate.size_bytes,
        });
    }
    if recovered < needed {
        return None;
    }
    Some(evictions)
}

fn used_bytes(state: &State, tier: &ResidencyTier) -> u64 {
    state
        .residents
        .values()
        .filter(|record| &record.tier == tier)
        .fold(0u64, |sum, record| sum.saturating_add(record.size_bytes))
}
