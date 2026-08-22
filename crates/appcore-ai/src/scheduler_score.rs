// =============================================================================
//        #######
//     ###       ###     F: scheduler_score.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{
    AiLatencyClass, AiPriority, AiResourceMode, BackendHealth, PlacementCandidate,
    PlacementContext, PlacementMetrics, PlacementRejectionReason, SchedulerWeights,
};
use std::time::Duration;

pub(crate) fn reject(
    context: PlacementContext,
    candidate: &PlacementCandidate,
    metrics: PlacementMetrics,
) -> Option<PlacementRejectionReason> {
    if candidate.health == BackendHealth::Unavailable {
        return Some(PlacementRejectionReason::Unavailable);
    }
    if candidate.key.target.is_remote()
        && (!context.allow_remote
            || !candidate.trusted
            || candidate
                .rtt_ms
                .is_none_or(|rtt| rtt > millis(context.max_remote_latency)))
    {
        return Some(PlacementRejectionReason::Policy);
    }
    if metrics
        .available_memory_bytes
        .is_some_and(|available| candidate.resources.memory_bytes > available)
    {
        return Some(PlacementRejectionReason::Memory);
    }
    if metrics
        .available_vram_bytes
        .is_some_and(|available| candidate.resources.vram_bytes > available)
    {
        return Some(PlacementRejectionReason::Vram);
    }
    let expected_ms = metrics
        .latency_ema_ms
        .unwrap_or_default()
        .saturating_add(if candidate.model_resident {
            0
        } else {
            candidate.load_time_ms
        })
        .saturating_add(candidate.rtt_ms.unwrap_or_default());
    if context
        .deadline_remaining
        .is_some_and(|deadline| expected_ms > millis(deadline))
    {
        return Some(PlacementRejectionReason::Deadline);
    }
    if context.pressure_limited
        && context.resource_mode != AiResourceMode::Unrestricted
        && metrics.load_percent.is_some_and(|load| load >= 90)
        && context.priority < AiPriority::Critical
    {
        return Some(PlacementRejectionReason::Pressure);
    }
    None
}

pub(crate) fn score(
    weights: SchedulerWeights,
    context: PlacementContext,
    candidate: &PlacementCandidate,
    metrics: PlacementMetrics,
) -> u64 {
    let (latency_multiplier, throughput_multiplier) = match context.latency_class {
        AiLatencyClass::Interactive => (4, 1),
        AiLatencyClass::Balanced => (2, 2),
        AiLatencyClass::Throughput => (1, 4),
        AiLatencyClass::Background => (1, 1),
    };
    let priority_divisor = match context.priority {
        AiPriority::Background => 1,
        AiPriority::Normal => 2,
        AiPriority::High => 4,
        AiPriority::Critical => 8,
    };
    let mode_divisor = match context.resource_mode {
        AiResourceMode::Eco => 1,
        AiResourceMode::Balanced | AiResourceMode::Custom(_) => 2,
        AiResourceMode::Performance => 4,
        AiResourceMode::Unrestricted => 8,
    };
    let mut total = u64::from(metrics.load_percent.unwrap_or(50)).saturating_mul(weights.load);
    total = total.saturating_add(
        u64::try_from(metrics.queue_depth)
            .unwrap_or(u64::MAX)
            .saturating_mul(weights.queue)
            / priority_divisor,
    );
    total = total.saturating_add(
        metrics
            .latency_ema_ms
            .unwrap_or(100)
            .saturating_mul(weights.latency)
            .saturating_mul(latency_multiplier)
            / priority_divisor,
    );
    total = total.saturating_add(if candidate.model_resident {
        0
    } else {
        candidate.load_time_ms.saturating_mul(weights.cold_load) / mode_divisor
    });
    total = total.saturating_add(
        candidate
            .transfer_cost_units
            .saturating_mul(weights.transfer),
    );
    total = total.saturating_add(
        candidate
            .inference_cost_units
            .saturating_mul(weights.inference),
    );
    total = total.saturating_add(
        candidate
            .rtt_ms
            .unwrap_or_default()
            .saturating_mul(weights.remote_latency)
            .saturating_mul(latency_multiplier),
    );
    if context.prefer_local && candidate.key.target.is_remote() {
        total = total.saturating_add(weights.local_preference);
    }
    total = total.saturating_add(
        candidate
            .failover_cost_units
            .saturating_mul(weights.failover),
    );
    if candidate.health == BackendHealth::Degraded {
        total = total.saturating_add(weights.degraded);
    }
    let residency_reward = if candidate.model_resident {
        weights.residency_reward
    } else {
        0
    };
    let reward = residency_reward.saturating_add(
        metrics
            .throughput_ema
            .unwrap_or_default()
            .saturating_mul(weights.throughput_reward)
            .saturating_mul(throughput_multiplier),
    );
    total.saturating_sub(reward)
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
