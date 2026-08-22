// =============================================================================
//        #######
//     ###       ###     F: execution_route.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{ArtifactLocation, ExecutionTarget, InferenceBackend, ModelRecord, PlacementKey};
use std::sync::Arc;

#[derive(Clone)]
pub(crate) enum ExecutionRoute {
    Local {
        key: PlacementKey,
        model: Arc<ModelRecord>,
        backend: Arc<dyn InferenceBackend>,
        device: crate::DeviceId,
    },
    #[cfg(feature = "swarm")]
    Remote {
        key: PlacementKey,
        route: Box<crate::SwarmRoute>,
    },
}

impl ExecutionRoute {
    pub(crate) fn key(&self) -> &PlacementKey {
        match self {
            Self::Local { key, .. } => key,
            #[cfg(feature = "swarm")]
            Self::Remote { key, .. } => key,
        }
    }

    pub(crate) fn target(&self) -> ExecutionTarget {
        match self {
            Self::Local {
                backend, device, ..
            } => ExecutionTarget::Local {
                backend: backend.descriptor().id.clone(),
                device: device.clone(),
            },
            #[cfg(feature = "swarm")]
            Self::Remote { route, .. } => ExecutionTarget::Remote {
                peer_class: route.peer_class.clone(),
                backend: route.backend.clone(),
                device: route.device.clone(),
            },
        }
    }

    pub(crate) fn device_kind(&self) -> Option<crate::DeviceKind> {
        match self.key().target {
            crate::ComputeTarget::LocalCpu(_) => Some(crate::DeviceKind::Cpu),
            crate::ComputeTarget::LocalGpu(_) => Some(crate::DeviceKind::Gpu),
            crate::ComputeTarget::LocalNpu(_) => Some(crate::DeviceKind::Npu),
            crate::ComputeTarget::RemotePeer { kind, .. } => Some(kind),
        }
    }
}

pub(crate) struct PlannedRoute {
    pub(crate) route: ExecutionRoute,
    pub(crate) score: u64,
}

pub(crate) fn model_is_resident(
    record: &ModelRecord,
    kind: crate::DeviceKind,
    device: &crate::DeviceId,
) -> bool {
    match kind {
        crate::DeviceKind::Cpu => record.locations.contains(&ArtifactLocation::Memory),
        crate::DeviceKind::Gpu | crate::DeviceKind::Npu => record
            .locations
            .contains(&ArtifactLocation::Vram(device.clone())),
    }
}

pub(crate) fn artifact_source(
    record: &ModelRecord,
    kind: crate::DeviceKind,
    device: &crate::DeviceId,
    allow_peer: bool,
) -> Option<ArtifactLocation> {
    let resident = match kind {
        crate::DeviceKind::Cpu => ArtifactLocation::Memory,
        crate::DeviceKind::Gpu | crate::DeviceKind::Npu => ArtifactLocation::Vram(device.clone()),
    };
    if record.locations.contains(&resident) {
        return Some(resident);
    }
    [ArtifactLocation::Memory, ArtifactLocation::LocalStorage]
        .into_iter()
        .find(|location| record.locations.contains(location))
        .or_else(|| {
            if allow_peer {
                record
                    .locations
                    .iter()
                    .find(|location| matches!(location, ArtifactLocation::Peer(_)))
                    .cloned()
            } else {
                None
            }
        })
}

#[cfg(feature = "swarm")]
pub(crate) fn remote_artifact_allowed(route: &crate::SwarmRoute, allow_peer: bool) -> bool {
    route.model_resident
        || !matches!(route.artifact_source, Some(ArtifactLocation::Peer(_)))
        || allow_peer
}
