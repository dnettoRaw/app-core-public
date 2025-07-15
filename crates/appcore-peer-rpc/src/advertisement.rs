// =============================================================================
//        #######
//     ###       ###     F: advertisement.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 13:45:20 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Conversion from core manifests to peer advertisement V1.

use super::*;
use appcore_core::{
    CapabilityDescriptor, CapabilityMode, CapabilityVisibility, CoreIdentity, PeerEndpoint,
};

pub(crate) fn advertisement_from_manifest(
    manifest: &DistributedCoreManifest,
) -> PeerAdvertisementV1 {
    PeerAdvertisementV1 {
        schema_version: 1,
        identity: identity_from_core(&manifest.identity),
        app_name: manifest.app_name.clone(),
        app_version: manifest.app_version.clone(),
        runtime_min_version: manifest.runtime_min_version.clone(),
        runtime_max_version: manifest.runtime_max_version.clone(),
        capabilities: manifest
            .capabilities
            .iter()
            .map(capability_from_core)
            .collect(),
        endpoints: manifest.endpoints.iter().map(endpoint_from_core).collect(),
        metadata: manifest.metadata.clone(),
    }
}

fn identity_from_core(identity: &CoreIdentity) -> PeerIdentityV1 {
    PeerIdentityV1 {
        tenant_id: identity.tenant_id.as_str().to_string(),
        cluster_id: identity.cluster_id.as_str().to_string(),
        core_id: identity.core_id.as_str().to_string(),
        instance_id: identity.instance_id.as_str().to_string(),
        kind: identity.kind.as_str().to_string(),
        protocol_version: identity.protocol_version.as_u16(),
        app_id: identity.runtime.app_id.as_str().to_string(),
        app_family: identity.runtime.app_family.as_str().to_string(),
        sync_group: identity.runtime.sync_group.as_str().to_string(),
        runtime_contract: identity.runtime.runtime_contract.as_u16(),
        node_id: identity.runtime.node_id.as_str().to_string(),
    }
}

fn capability_from_core(capability: &CapabilityDescriptor) -> PeerCapabilityV1 {
    PeerCapabilityV1 {
        name: capability.name.as_str().to_string(),
        version: capability.version.clone(),
        mode: capability_mode(capability.mode).to_string(),
        visibility: capability_visibility(capability.visibility).to_string(),
        requires_leader: capability.requirements.requires_leader,
        read_only: capability.requirements.read_only,
        idempotency_required: capability.requirements.idempotency_required,
    }
}

fn endpoint_from_core(endpoint: &PeerEndpoint) -> PeerEndpointV1 {
    PeerEndpointV1 {
        name: endpoint.name.clone(),
        url: endpoint.url.clone(),
        protocol: endpoint.protocol.clone(),
        metadata: endpoint.metadata.clone(),
    }
}

fn capability_mode(mode: CapabilityMode) -> &'static str {
    match mode {
        CapabilityMode::Query => "query",
        CapabilityMode::Command => "command",
        CapabilityMode::Stream => "stream",
    }
}

fn capability_visibility(visibility: CapabilityVisibility) -> &'static str {
    match visibility {
        CapabilityVisibility::Local => "local",
        CapabilityVisibility::Cluster => "cluster",
        CapabilityVisibility::Tenant => "tenant",
    }
}
