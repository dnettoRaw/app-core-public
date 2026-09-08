// =============================================================================
//        #######
//     ###       ###     F: accounting.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/02 22:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/02 22:00:00 by dnettoRaw
//      ###########      S: 1.0.3-rc
// =============================================================================

//! Estimates heap retained by decoded control-plane state.

use super::{InMemoryState, ServiceLeaseSlot};
use crate::{CoreIdentity, CoreRegistration, ServiceLeaderLease};
use appcore_core::{CapabilityDescriptor, PeerEndpoint};
use std::collections::BTreeMap;

pub(super) fn state_retained_bytes(state: &InMemoryState) -> usize {
    state
        .registrations
        .iter()
        .fold(0usize, |total, (key, registration)| {
            total.saturating_add(registration_retained_bytes(key, registration))
        })
        .saturating_add(
            state
                .service_leases
                .iter()
                .fold(0usize, |total, (key, slot)| {
                    total.saturating_add(lease_slot_retained_bytes(key, slot))
                }),
        )
}

pub(super) fn registration_retained_bytes(key: &String, registration: &CoreRegistration) -> usize {
    const ENTRY_OVERHEAD: usize =
        std::mem::size_of::<(String, CoreRegistration)>() + std::mem::size_of::<usize>() * 4;
    let manifest = &registration.manifest;
    ENTRY_OVERHEAD
        .saturating_add(key.capacity())
        .saturating_add(identity_dynamic_bytes(&manifest.identity))
        .saturating_add(manifest.app_name.capacity())
        .saturating_add(manifest.app_version.capacity())
        .saturating_add(manifest.runtime_min_version.capacity())
        .saturating_add(
            manifest
                .runtime_max_version
                .as_ref()
                .map_or(0, String::capacity),
        )
        .saturating_add(
            manifest
                .capabilities
                .capacity()
                .saturating_mul(std::mem::size_of::<CapabilityDescriptor>()),
        )
        .saturating_add(capability_dynamic_bytes(&manifest.capabilities))
        .saturating_add(
            manifest
                .endpoints
                .capacity()
                .saturating_mul(std::mem::size_of::<PeerEndpoint>()),
        )
        .saturating_add(endpoint_dynamic_bytes(&manifest.endpoints))
        .saturating_add(metadata_retained_bytes(&manifest.metadata))
}

pub(super) fn lease_slot_retained_bytes(key: &String, slot: &ServiceLeaseSlot) -> usize {
    const ENTRY_OVERHEAD: usize =
        std::mem::size_of::<(String, ServiceLeaseSlot)>() + std::mem::size_of::<usize>() * 4;
    ENTRY_OVERHEAD
        .saturating_add(key.capacity())
        .saturating_add(slot.lease.as_ref().map_or(0, lease_dynamic_bytes))
}

fn identity_dynamic_bytes(identity: &CoreIdentity) -> usize {
    identity
        .tenant_id
        .as_str()
        .len()
        .saturating_add(identity.cluster_id.as_str().len())
        .saturating_add(identity.core_id.as_str().len())
        .saturating_add(identity.instance_id.as_str().len())
        .saturating_add(identity.kind.as_str().len())
        .saturating_add(identity.runtime.app_id.as_str().len())
        .saturating_add(identity.runtime.app_family.as_str().len())
        .saturating_add(identity.runtime.sync_group.as_str().len())
        .saturating_add(identity.runtime.node_id.as_str().len())
}

fn capability_dynamic_bytes(capabilities: &[CapabilityDescriptor]) -> usize {
    capabilities.iter().fold(0usize, |total, capability| {
        total
            .saturating_add(capability.name.as_str().len())
            .saturating_add(capability.version.capacity())
    })
}

fn endpoint_dynamic_bytes(endpoints: &[PeerEndpoint]) -> usize {
    endpoints.iter().fold(0usize, |total, endpoint| {
        total
            .saturating_add(endpoint.name.capacity())
            .saturating_add(endpoint.url.capacity())
            .saturating_add(endpoint.protocol.capacity())
            .saturating_add(metadata_retained_bytes(&endpoint.metadata))
    })
}

fn metadata_retained_bytes(metadata: &BTreeMap<String, String>) -> usize {
    const ENTRY_OVERHEAD: usize =
        std::mem::size_of::<(String, String)>() + std::mem::size_of::<usize>() * 4;
    metadata.iter().fold(0usize, |total, (key, value)| {
        total
            .saturating_add(ENTRY_OVERHEAD)
            .saturating_add(key.capacity())
            .saturating_add(value.capacity())
    })
}

fn lease_dynamic_bytes(lease: &ServiceLeaderLease) -> usize {
    lease
        .service_id
        .as_str()
        .len()
        .saturating_add(lease.tenant_id.as_str().len())
        .saturating_add(lease.cluster_id.as_str().len())
        .saturating_add(lease.holder_core_id.as_str().len())
}
