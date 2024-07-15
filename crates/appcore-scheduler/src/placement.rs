// =============================================================================
//        #######
//     ###       ###     F: placement.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/21 23:21:21 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Explainable capability placement for standalone and cluster runtimes.

use appcore_contracts::{
    CapabilityId, CoreId, CoreProfile, RuntimeHealthStatus, RuntimeMode, RuntimeOperationalMode,
    ServiceId, WorkloadClass,
};
use std::collections::BTreeSet;

/// Minimum resources requested by one placement operation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResourceRequest {
    /// Minimum logical CPU cores.
    pub cpu_cores: Option<u16>,
    /// Minimum memory in bytes.
    pub memory_bytes: Option<u64>,
    /// Minimum GPU devices.
    pub gpu_count: u16,
}

/// Inputs required to place one capability invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacementRequest {
    /// Capability required by the operation.
    pub capability: CapabilityId,
    /// Independently coordinated service.
    pub service_id: ServiceId,
    /// Required standalone or cluster mode.
    pub runtime_mode: RuntimeMode,
    /// Whether the operation writes state.
    pub requires_write: bool,
    /// Whether service leadership is required.
    pub requires_leader: bool,
    /// Preferred workload class.
    pub workload: WorkloadClass,
    /// Required scheduling affinity labels.
    pub affinity: BTreeSet<String>,
    /// Minimum resources.
    pub resources: ResourceRequest,
}

/// Runtime state advertised by one placement candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacementCandidate {
    /// Candidate Core identity.
    pub core_id: CoreId,
    /// Candidate Runtime mode.
    pub runtime_mode: RuntimeMode,
    /// Current operational mode.
    pub operational_mode: RuntimeOperationalMode,
    /// Current health.
    pub health: RuntimeHealthStatus,
    /// Current active-work count.
    pub current_load: u32,
    /// Services for which the Core holds leadership.
    pub leader_services: BTreeSet<ServiceId>,
    /// Declared Core profile.
    pub profile: CoreProfile,
}

/// Stable reason that made a placement candidate ineligible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlacementRejection {
    /// Required capability is absent.
    CapabilityUnavailable,
    /// Standalone/cluster mode differs.
    RuntimeModeMismatch,
    /// Candidate is unhealthy.
    Unhealthy,
    /// Operational mode disallows the requested work.
    OperationalMode,
    /// Scheduling profile marks the candidate unavailable.
    Unavailable,
    /// Required service lease is absent.
    LeadershipRequired,
    /// Concurrency capacity is exhausted.
    AtCapacity,
    /// Required affinity labels are absent.
    AffinityMismatch,
    /// CPU request exceeds the profile.
    InsufficientCpu,
    /// Memory request exceeds the profile.
    InsufficientMemory,
    /// GPU request exceeds the profile.
    InsufficientGpu,
}

/// Explainable evaluation of one candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacementEvaluation {
    /// Evaluated Core.
    pub core_id: CoreId,
    /// Whether all hard constraints passed.
    pub eligible: bool,
    /// Deterministic score for an eligible candidate.
    pub score: i64,
    /// First hard-constraint failure.
    pub rejection: Option<PlacementRejection>,
}

/// Complete placement result, including rejected alternatives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacementDecision {
    /// Highest-scoring eligible Core.
    pub selected_core_id: Option<CoreId>,
    /// Evaluation of every candidate.
    pub evaluations: Vec<PlacementEvaluation>,
}

/// Deterministic scheduler policy that evaluates all foundation signals.
#[derive(Debug, Clone, Copy, Default)]
pub struct PlacementEngine;

impl PlacementEngine {
    /// Evaluates candidates and selects the highest-scoring eligible core.
    pub fn select(
        &self,
        request: &PlacementRequest,
        candidates: &[PlacementCandidate],
    ) -> PlacementDecision {
        let mut evaluations = candidates
            .iter()
            .map(|candidate| evaluate_candidate(request, candidate))
            .collect::<Vec<_>>();
        evaluations.sort_by(|left, right| left.core_id.cmp(&right.core_id));
        let selected_core_id = evaluations
            .iter()
            .filter(|evaluation| evaluation.eligible)
            .max_by(|left, right| {
                left.score
                    .cmp(&right.score)
                    .then_with(|| right.core_id.cmp(&left.core_id))
            })
            .map(|evaluation| evaluation.core_id.clone());
        PlacementDecision {
            selected_core_id,
            evaluations,
        }
    }
}

fn evaluate_candidate(
    request: &PlacementRequest,
    candidate: &PlacementCandidate,
) -> PlacementEvaluation {
    let rejection = rejection_reason(request, candidate);
    if let Some(rejection) = rejection {
        return PlacementEvaluation {
            core_id: candidate.core_id.clone(),
            eligible: false,
            score: i64::MIN,
            rejection: Some(rejection),
        };
    }
    let scheduling = candidate.profile.scheduling();
    let health_score = match candidate.health {
        RuntimeHealthStatus::Healthy => 2_000,
        RuntimeHealthStatus::Degraded => 250,
        RuntimeHealthStatus::Unhealthy => 0,
    };
    let workload_score = if scheduling.workload() == request.workload {
        1_000
    } else {
        0
    };
    let affinity_score = i64::try_from(request.affinity.len()).unwrap_or(i64::MAX / 100) * 100;
    let spare_capacity = scheduling
        .max_concurrency()
        .saturating_sub(candidate.current_load);
    let score = i64::from(scheduling.priority()) * 10_000
        + i64::from(scheduling.weight()) * 100
        + i64::from(spare_capacity) * 10
        + health_score
        + workload_score
        + affinity_score;
    PlacementEvaluation {
        core_id: candidate.core_id.clone(),
        eligible: true,
        score,
        rejection: None,
    }
}

fn rejection_reason(
    request: &PlacementRequest,
    candidate: &PlacementCandidate,
) -> Option<PlacementRejection> {
    if !candidate
        .profile
        .capabilities()
        .contains(&request.capability)
    {
        return Some(PlacementRejection::CapabilityUnavailable);
    }
    if candidate.runtime_mode != request.runtime_mode {
        return Some(PlacementRejection::RuntimeModeMismatch);
    }
    if candidate.health == RuntimeHealthStatus::Unhealthy {
        return Some(PlacementRejection::Unhealthy);
    }
    if !candidate.operational_mode.allows_local_queries()
        || (request.requires_write && !candidate.operational_mode.allows_writes())
    {
        return Some(PlacementRejection::OperationalMode);
    }
    let scheduling = candidate.profile.scheduling();
    if !scheduling.is_available() {
        return Some(PlacementRejection::Unavailable);
    }
    if request.requires_leader && !candidate.leader_services.contains(&request.service_id) {
        return Some(PlacementRejection::LeadershipRequired);
    }
    if candidate.current_load >= scheduling.max_concurrency() {
        return Some(PlacementRejection::AtCapacity);
    }
    if !request.affinity.is_subset(scheduling.affinity()) {
        return Some(PlacementRejection::AffinityMismatch);
    }
    let resources = candidate.profile.resources();
    if request.resources.cpu_cores.is_some_and(|required| {
        resources
            .cpu_cores()
            .is_none_or(|available| available < required)
    }) {
        return Some(PlacementRejection::InsufficientCpu);
    }
    if request.resources.memory_bytes.is_some_and(|required| {
        resources
            .memory_bytes()
            .is_none_or(|available| available < required)
    }) {
        return Some(PlacementRejection::InsufficientMemory);
    }
    if resources.gpu_count() < request.resources.gpu_count {
        return Some(PlacementRejection::InsufficientGpu);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use appcore_contracts::{
        CoreRole, LeadershipMode, LeadershipRequirement, ResourceProfile, SchedulingProfile,
    };

    fn request() -> PlacementRequest {
        PlacementRequest {
            capability: CapabilityId::new("document.extract").unwrap(),
            service_id: ServiceId::new("document.extract").unwrap(),
            runtime_mode: RuntimeMode::Cluster,
            requires_write: true,
            requires_leader: true,
            workload: WorkloadClass::Compute,
            affinity: BTreeSet::from(["region.local".to_string()]),
            resources: ResourceRequest {
                cpu_cores: Some(4),
                memory_bytes: Some(8_000),
                gpu_count: 1,
            },
        }
    }

    fn candidate(core: &str, weight: u16, load: u32) -> PlacementCandidate {
        let service = ServiceId::new("document.extract").unwrap();
        let scheduling = SchedulingProfile::new(weight, 1, 8, WorkloadClass::Compute)
            .unwrap()
            .with_affinity("region.local")
            .unwrap();
        let profile = CoreProfile::new(
            CoreRole::Compute,
            service.clone(),
            [CapabilityId::new("document.extract").unwrap()],
            LeadershipRequirement::new(service.clone(), LeadershipMode::Required, 30_000).unwrap(),
            ResourceProfile::new(Some(8), Some(16_000), 1),
            scheduling,
        )
        .unwrap();
        PlacementCandidate {
            core_id: CoreId::new(core).unwrap(),
            runtime_mode: RuntimeMode::Cluster,
            operational_mode: RuntimeOperationalMode::ReadWrite,
            health: RuntimeHealthStatus::Healthy,
            current_load: load,
            leader_services: BTreeSet::from([service]),
            profile,
        }
    }

    #[test]
    fn placement_uses_weight_and_current_load() {
        let decision = PlacementEngine.select(
            &request(),
            &[candidate("core-a", 10, 7), candidate("core-b", 20, 1)],
        );
        assert_eq!(decision.selected_core_id.unwrap().as_str(), "core-b");
    }

    #[test]
    fn placement_rejects_missing_leadership() {
        let mut candidate = candidate("core-a", 10, 0);
        candidate.leader_services.clear();
        let decision = PlacementEngine.select(&request(), &[candidate]);
        assert!(decision.selected_core_id.is_none());
        assert_eq!(
            decision.evaluations[0].rejection,
            Some(PlacementRejection::LeadershipRequired)
        );
    }

    #[test]
    fn placement_rejects_resource_and_affinity_mismatches() {
        let mut resources = candidate("core-a", 10, 0);
        let mut affinity = candidate("core-b", 10, 0);
        resources.profile = CoreProfile::new(
            CoreRole::Compute,
            ServiceId::new("document.extract").unwrap(),
            [CapabilityId::new("document.extract").unwrap()],
            LeadershipRequirement::new(
                ServiceId::new("document.extract").unwrap(),
                LeadershipMode::Required,
                30_000,
            )
            .unwrap(),
            ResourceProfile::new(Some(2), Some(16_000), 1),
            SchedulingProfile::new(10, 1, 8, WorkloadClass::Compute)
                .unwrap()
                .with_affinity("region.local")
                .unwrap(),
        )
        .unwrap();
        affinity.profile = CoreProfile::new(
            CoreRole::Compute,
            ServiceId::new("document.extract").unwrap(),
            [CapabilityId::new("document.extract").unwrap()],
            LeadershipRequirement::new(
                ServiceId::new("document.extract").unwrap(),
                LeadershipMode::Required,
                30_000,
            )
            .unwrap(),
            ResourceProfile::new(Some(8), Some(16_000), 1),
            SchedulingProfile::new(10, 1, 8, WorkloadClass::Compute).unwrap(),
        )
        .unwrap();
        let decision = PlacementEngine.select(&request(), &[resources, affinity]);
        assert_eq!(
            decision.evaluations[0].rejection,
            Some(PlacementRejection::InsufficientCpu)
        );
        assert_eq!(
            decision.evaluations[1].rejection,
            Some(PlacementRejection::AffinityMismatch)
        );
    }
}
