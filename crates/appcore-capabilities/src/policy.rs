// =============================================================================
//        #######
//     ###       ###     F: policy.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/20 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/20 00:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use crate::{CapabilityError, CapabilityRequest, CapabilityResult};
use appcore_contracts::ServiceId;
use appcore_core::{CapabilityDescriptor, CapabilityMode, CoreId, CoreIdentity};
use appcore_distributed_contracts::{LeadershipDecision, ServiceLeadershipGuard};

#[allow(clippy::too_many_arguments)]
pub(crate) fn enforce_requirements(
    identity: &CoreIdentity,
    service_id: &ServiceId,
    request: &CapabilityRequest,
    descriptor: &CapabilityDescriptor,
    provider_core_id: &CoreId,
    leadership: Option<&dyn ServiceLeadershipGuard>,
    writes_allowed: bool,
    now_ms: u64,
) -> CapabilityResult<()> {
    enforce_request_requirements(request, descriptor)?;
    if request.mode == CapabilityMode::Command && !writes_allowed {
        return Err(CapabilityError::WritesDisabled(request.capability.clone()));
    }
    if !descriptor.requirements.requires_leader {
        return Ok(());
    }
    let Some(leadership) = leadership else {
        return Err(CapabilityError::RequiresLeader(request.capability.clone()));
    };
    map_leadership_decision(
        request,
        leadership.check_service_write_permission(
            service_id,
            &identity.tenant_id,
            &identity.cluster_id,
            provider_core_id,
            None,
            now_ms,
        ),
    )
}

pub(crate) fn enforce_request_requirements(
    request: &CapabilityRequest,
    descriptor: &CapabilityDescriptor,
) -> CapabilityResult<()> {
    let requirements = descriptor.requirements;
    if descriptor.mode != request.mode {
        return Err(CapabilityError::HandlerRejected(
            "capability_mode_mismatch".to_string(),
        ));
    }
    if requirements.read_only && request.mode != CapabilityMode::Query {
        return Err(CapabilityError::HandlerRejected(
            "read_only_capability".to_string(),
        ));
    }
    if requirements.idempotency_required && request.idempotency_key.is_none() {
        return Err(CapabilityError::HandlerRejected(
            "missing_idempotency_key".to_string(),
        ));
    }
    Ok(())
}

fn map_leadership_decision(
    request: &CapabilityRequest,
    decision: LeadershipDecision,
) -> CapabilityResult<()> {
    match decision {
        LeadershipDecision::Allowed => Ok(()),
        LeadershipDecision::Expired => {
            Err(CapabilityError::LeaseExpired(request.capability.clone()))
        }
        LeadershipDecision::StaleEpoch => {
            Err(CapabilityError::StaleEpoch(request.capability.clone()))
        }
        LeadershipDecision::NoLease | LeadershipDecision::WrongHolder => {
            Err(CapabilityError::RequiresLeader(request.capability.clone()))
        }
    }
}
