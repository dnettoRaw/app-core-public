// =============================================================================
//        #######
//     ###       ###     F: tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;
use appcore_contracts::ServiceId;
use appcore_control_plane::{PeerRecord, ServiceLeaderLease, StaticServiceLeadershipGuard};
use appcore_core::{
    AppFamily, AppId, CapabilityDescriptor, CapabilityMode, CapabilityName, CapabilityVisibility,
    ClusterId, CoreId, CoreIdentity, CoreKind, InstanceId, NodeId, PeerEndpoint, ProtocolVersion,
    RuntimeContractVersion, RuntimeIdentity, SyncGroup, TenantId, TraceContext,
};
use appcore_peer_rpc::{
    PeerRpcCallKind, PeerRpcClientExecutor, PeerRpcError, PeerRpcOutboundRequest, PeerRpcResponse,
};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct EchoHandler {
    descriptor: CapabilityDescriptor,
}

#[test]
fn local_registry_bounds_metadata_and_borrows_descriptors() {
    let mut registry = CapabilityRegistry::new();
    let mut invalid = descriptor("runtime.invalid");
    invalid.version = "x".repeat(257);
    assert!(registry
        .register_handler(EchoHandler {
            descriptor: invalid
        })
        .is_err());
    assert_eq!(registry.iter_descriptors().len(), 0);
    for index in 0..4096 {
        registry
            .register_handler(EchoHandler {
                descriptor: descriptor(&format!("runtime.item_{index}")),
            })
            .unwrap();
    }
    assert!(registry
        .register_handler(EchoHandler {
            descriptor: descriptor("runtime.overflow")
        })
        .is_err());
    assert_eq!(registry.iter_descriptors().len(), 4096);
    let borrowed = registry.iter_descriptors().next().unwrap();
    assert!(std::ptr::eq(
        borrowed,
        registry.get(&borrowed.name).unwrap().descriptor()
    ));
}

struct FakePeerRpcClient;

#[test]
fn descriptor_catalog_stops_ingestion_at_capacity() {
    let consumed = AtomicUsize::new(0);
    let input = (0..5000).map(|index| {
        consumed.fetch_add(1, Ordering::SeqCst);
        descriptor(&format!("runtime.item_{index}"))
    });
    assert!(CapabilityCatalog::from_descriptors(input).is_err());
    assert_eq!(consumed.load(Ordering::SeqCst), 4097);
    let mut catalog = CapabilityCatalog::new();
    let mut invalid = descriptor("runtime.invalid");
    invalid.version.clear();
    assert!(catalog.register_descriptor(invalid).is_err());
    assert!(catalog.descriptors().is_empty());
}

struct OwnedPeerRpcClient {
    payload_pointer: usize,
}

struct SelectedPeerInvoker {
    expected_core_id: &'static str,
    expected_peer_pointer: Option<usize>,
}

struct LastCandidateSelector {
    seen_candidates: Arc<AtomicUsize>,
}

impl CapabilitySelectionPolicy for LastCandidateSelector {
    fn select(&self, candidates: &[CapabilityProvider]) -> Option<CapabilityProvider> {
        self.seen_candidates
            .store(candidates.len(), Ordering::Release);
        candidates.last().cloned()
    }
}

impl LocalCapabilityHandler for EchoHandler {
    fn descriptor(&self) -> CapabilityDescriptor {
        self.descriptor.clone()
    }

    fn handle(&self, request: &CapabilityRequest) -> CapabilityResult<CapabilityResponse> {
        Ok(CapabilityResponse::accepted(request.payload.clone(), None))
    }
}

impl PeerRpcClientExecutor for FakePeerRpcClient {
    fn call_peer(
        &self,
        endpoint_url: &str,
        kind: PeerRpcCallKind,
        request: PeerRpcOutboundRequest,
    ) -> Result<PeerRpcResponse, PeerRpcError> {
        assert_eq!(endpoint_url, "http://127.0.0.1:39301");
        assert_eq!(kind, PeerRpcCallKind::Query);
        Ok(PeerRpcResponse::ok(request.request_id, request.payload))
    }
}

impl PeerRpcClientExecutor for OwnedPeerRpcClient {
    fn call_peer(
        &self,
        endpoint_url: &str,
        kind: PeerRpcCallKind,
        request: PeerRpcOutboundRequest,
    ) -> Result<PeerRpcResponse, PeerRpcError> {
        assert_eq!(endpoint_url, "http://127.0.0.1:39301");
        assert_eq!(kind, PeerRpcCallKind::Query);
        assert_eq!(request.request_id, "req-owned");
        assert_eq!(request.capability.as_str(), "runtime.echo");
        assert_eq!(request.payload.as_ptr() as usize, self.payload_pointer);
        assert_eq!(request.idempotency_key.as_deref(), Some("idem-owned"));
        assert_eq!(
            request.trace.as_ref().map(|trace| trace.trace_id.as_str()),
            Some("trace-owned")
        );
        Ok(PeerRpcResponse::ok(request.request_id, request.payload))
    }
}

impl RemoteCapabilityInvoker for SelectedPeerInvoker {
    fn invoke_remote(
        &self,
        peer: &PeerRecord,
        request: &CapabilityRequest,
    ) -> CapabilityResult<CapabilityResponse> {
        assert_eq!(peer.identity.core_id.as_str(), self.expected_core_id);
        if let Some(expected) = self.expected_peer_pointer {
            assert_eq!(peer as *const PeerRecord as usize, expected);
        }
        Ok(CapabilityResponse::accepted(request.payload.clone(), None))
    }
}

fn identity(core_id: &str) -> CoreIdentity {
    CoreIdentity {
        tenant_id: TenantId::new("tenant-a").unwrap(),
        cluster_id: ClusterId::new("cluster-a").unwrap(),
        core_id: CoreId::new(core_id).unwrap(),
        instance_id: InstanceId::new(format!("{core_id}-instance")).unwrap(),
        kind: CoreKind::operational(),
        protocol_version: ProtocolVersion::new(1),
        runtime: RuntimeIdentity {
            app_id: AppId::new("app-a").unwrap(),
            app_family: AppFamily::new("family-a").unwrap(),
            sync_group: SyncGroup::new("cluster-a").unwrap(),
            runtime_contract: RuntimeContractVersion::new(1),
            node_id: NodeId::new(core_id).unwrap(),
        },
    }
}

fn descriptor(name: &str) -> CapabilityDescriptor {
    CapabilityDescriptor::new(
        CapabilityName::new(name).unwrap(),
        "1",
        CapabilityMode::Query,
        CapabilityVisibility::Cluster,
    )
}

fn request(name: &str) -> CapabilityRequest {
    CapabilityRequest {
        request_id: "req-1".to_string(),
        capability: CapabilityName::new(name).unwrap(),
        mode: CapabilityMode::Query,
        payload: b"hello".to_vec(),
        idempotency_key: None,
        trace: None,
    }
}

fn service_id() -> ServiceId {
    ServiceId::new("runtime.query").unwrap()
}

fn peer(core_id: &str, descriptor: CapabilityDescriptor, preferred: bool) -> PeerRecord {
    let mut metadata = BTreeMap::new();
    if preferred {
        metadata.insert("preferred".to_string(), "true".to_string());
    }
    PeerRecord {
        identity: identity(core_id),
        endpoints: Vec::new(),
        capabilities: vec![descriptor],
        healthy: true,
        last_seen_ms: 10,
        metadata,
    }
}

fn peer_with_rpc_endpoint(
    core_id: &str,
    descriptor: CapabilityDescriptor,
    preferred: bool,
) -> PeerRecord {
    let mut peer = peer(core_id, descriptor, preferred);
    peer.endpoints.push(PeerEndpoint {
        name: "peer-rpc".to_string(),
        url: "http://127.0.0.1:39301".to_string(),
        protocol: "appcore-peer-rpc".to_string(),
        metadata: BTreeMap::new(),
    });
    peer
}

#[test]
fn resolves_local_provider_first() {
    let mut registry = CapabilityRegistry::new();
    registry
        .register_handler(EchoHandler {
            descriptor: descriptor("runtime.echo"),
        })
        .unwrap();
    let resolver = CapabilityResolver::new(registry);
    let provider = resolver
        .resolve(
            &identity("core-a"),
            &service_id(),
            &request("runtime.echo"),
            None,
            0,
        )
        .unwrap();

    assert!(!provider.is_remote());
}

#[test]
fn resolves_remote_provider_when_local_missing() {
    let resolver = CapabilityResolver::new(CapabilityRegistry::new()).with_peers(vec![peer(
        "core-b",
        descriptor("runtime.echo"),
        false,
    )]);
    let provider = resolver
        .resolve(
            &identity("core-a"),
            &service_id(),
            &request("runtime.echo"),
            None,
            0,
        )
        .unwrap();

    assert!(provider.is_remote());
    assert_eq!(provider.core_id().as_str(), "core-b");
}

#[test]
fn resolves_preferred_remote_provider_before_other_remote() {
    let resolver = CapabilityResolver::new(CapabilityRegistry::new()).with_peers(vec![
        peer("core-b", descriptor("runtime.echo"), false),
        peer("core-c", descriptor("runtime.echo"), true),
    ]);
    let provider = resolver
        .resolve(
            &identity("core-a"),
            &service_id(),
            &request("runtime.echo"),
            None,
            0,
        )
        .unwrap();

    assert_eq!(provider.core_id().as_str(), "core-c");
}

#[test]
fn default_remote_fallback_preserves_discovery_order() {
    let resolver = CapabilityResolver::new(CapabilityRegistry::new()).with_peers(vec![
        peer("core-b", descriptor("runtime.echo"), false),
        peer("core-c", descriptor("runtime.echo"), false),
    ]);
    let provider = resolver
        .resolve(
            &identity("core-a"),
            &service_id(),
            &request("runtime.echo"),
            None,
            0,
        )
        .unwrap();

    assert_eq!(provider.core_id().as_str(), "core-b");
}

#[test]
fn custom_selector_receives_every_compatible_candidate() {
    let seen_candidates = Arc::new(AtomicUsize::new(0));
    let resolver = CapabilityResolver::new(CapabilityRegistry::new())
        .with_peers(vec![
            peer("core-b", descriptor("runtime.echo"), false),
            peer("core-c", descriptor("runtime.echo"), false),
        ])
        .with_selector(Arc::new(LastCandidateSelector {
            seen_candidates: Arc::clone(&seen_candidates),
        }));
    let provider = resolver
        .resolve(
            &identity("core-a"),
            &service_id(),
            &request("runtime.echo"),
            None,
            0,
        )
        .unwrap();

    assert_eq!(provider.core_id().as_str(), "core-c");
    assert_eq!(seen_candidates.load(Ordering::Acquire), 2);
}

#[test]
fn default_dispatch_borrows_the_selected_discovery_record() {
    let peers = vec![peer("core-b", descriptor("runtime.echo"), false)];
    let peer_pointer = peers.as_ptr() as usize;
    let resolver = CapabilityResolver::new(CapabilityRegistry::new()).with_peers(peers);
    let invoker = SelectedPeerInvoker {
        expected_core_id: "core-b",
        expected_peer_pointer: Some(peer_pointer),
    };

    let response = resolver
        .handle(
            &identity("core-a"),
            &service_id(),
            &request("runtime.echo"),
            None,
            Some(&invoker),
            0,
        )
        .unwrap();

    assert!(response.accepted);
}

#[test]
fn custom_selector_dispatch_keeps_the_owned_candidate_contract() {
    let seen_candidates = Arc::new(AtomicUsize::new(0));
    let resolver = CapabilityResolver::new(CapabilityRegistry::new())
        .with_peers(vec![
            peer("core-b", descriptor("runtime.echo"), false),
            peer("core-c", descriptor("runtime.echo"), false),
        ])
        .with_selector(Arc::new(LastCandidateSelector {
            seen_candidates: Arc::clone(&seen_candidates),
        }));
    let invoker = SelectedPeerInvoker {
        expected_core_id: "core-c",
        expected_peer_pointer: None,
    };

    let response = resolver
        .handle(
            &identity("core-a"),
            &service_id(),
            &request("runtime.echo"),
            None,
            Some(&invoker),
            0,
        )
        .unwrap();

    assert!(response.accepted);
    assert_eq!(seen_candidates.load(Ordering::Acquire), 2);
}

#[test]
fn reports_capability_unavailable() {
    let resolver = CapabilityResolver::new(CapabilityRegistry::new());
    assert!(matches!(
        resolver.resolve(
            &identity("core-a"),
            &service_id(),
            &request("runtime.echo"),
            None,
            0
        ),
        Err(CapabilityError::ProviderUnavailable(_))
    ));
}

#[test]
fn capability_that_requires_leader_is_rejected_without_lease() {
    let mut descriptor = descriptor("runtime.write");
    descriptor.mode = CapabilityMode::Command;
    descriptor.requirements.requires_leader = true;
    let mut registry = CapabilityRegistry::new();
    registry
        .register_handler(EchoHandler {
            descriptor: descriptor.clone(),
        })
        .unwrap();
    let mut request = request("runtime.write");
    request.mode = CapabilityMode::Command;
    let resolver = CapabilityResolver::new(registry);

    assert!(matches!(
        resolver.resolve(&identity("core-a"), &service_id(), &request, None, 0),
        Err(CapabilityError::RequiresLeader(_))
    ));
}

#[test]
fn capability_that_requires_leader_accepts_valid_lease() {
    let core = identity("core-a");
    let mut descriptor = descriptor("runtime.write");
    descriptor.mode = CapabilityMode::Command;
    descriptor.requirements.requires_leader = true;
    let mut registry = CapabilityRegistry::new();
    registry
        .register_handler(EchoHandler {
            descriptor: descriptor.clone(),
        })
        .unwrap();
    let service = service_id();
    let guard = StaticServiceLeadershipGuard::new([ServiceLeaderLease {
        service_id: service.clone(),
        tenant_id: core.tenant_id.clone(),
        cluster_id: core.cluster_id.clone(),
        holder_core_id: core.core_id.clone(),
        epoch: 1,
        acquired_at_ms: 0,
        expires_at_ms: 100,
    }]);
    let mut request = request("runtime.write");
    request.mode = CapabilityMode::Command;
    let resolver = CapabilityResolver::new(registry);

    assert!(resolver
        .resolve(&core, &service, &request, Some(&guard), 10)
        .is_ok());
}

#[test]
fn service_scoped_resolution_rejects_a_lease_for_another_service() {
    let core = identity("core-a");
    let mut descriptor = descriptor("runtime.write");
    descriptor.mode = CapabilityMode::Command;
    descriptor.requirements.requires_leader = true;
    let mut registry = CapabilityRegistry::new();
    registry
        .register_handler(EchoHandler {
            descriptor: descriptor.clone(),
        })
        .unwrap();
    let guard = StaticServiceLeadershipGuard::new([ServiceLeaderLease {
        service_id: ServiceId::new("service-b").unwrap(),
        tenant_id: core.tenant_id.clone(),
        cluster_id: core.cluster_id.clone(),
        holder_core_id: core.core_id.clone(),
        epoch: 1,
        acquired_at_ms: 0,
        expires_at_ms: 100,
    }]);
    let mut request = request("runtime.write");
    request.mode = CapabilityMode::Command;
    let resolver = CapabilityResolver::new(registry);

    assert!(matches!(
        resolver.resolve(
            &core,
            &ServiceId::new("service-a").unwrap(),
            &request,
            Some(&guard),
            10,
        ),
        Err(CapabilityError::RequiresLeader(_))
    ));
}

#[test]
fn service_scoped_resolution_accepts_the_matching_service_lease() {
    let core = identity("core-a");
    let service_id = ServiceId::new("service-a").unwrap();
    let mut descriptor = descriptor("runtime.write");
    descriptor.mode = CapabilityMode::Command;
    descriptor.requirements.requires_leader = true;
    let mut registry = CapabilityRegistry::new();
    registry
        .register_handler(EchoHandler {
            descriptor: descriptor.clone(),
        })
        .unwrap();
    let guard = StaticServiceLeadershipGuard::new([ServiceLeaderLease {
        service_id: service_id.clone(),
        tenant_id: core.tenant_id.clone(),
        cluster_id: core.cluster_id.clone(),
        holder_core_id: core.core_id.clone(),
        epoch: 1,
        acquired_at_ms: 0,
        expires_at_ms: 100,
    }]);
    let mut request = request("runtime.write");
    request.mode = CapabilityMode::Command;
    let resolver = CapabilityResolver::new(registry);

    assert!(resolver
        .resolve(&core, &service_id, &request, Some(&guard), 10)
        .is_ok());
}

#[test]
fn invokes_remote_provider_through_peer_rpc_invoker() {
    let resolver = CapabilityResolver::new(CapabilityRegistry::new()).with_peers(vec![
        peer_with_rpc_endpoint("core-b", descriptor("runtime.echo"), false),
    ]);
    let invoker = PeerRpcRemoteCapabilityInvoker::new(FakePeerRpcClient);
    let response = resolver
        .handle(
            &identity("core-a"),
            &service_id(),
            &request("runtime.echo"),
            None,
            Some(&invoker),
            0,
        )
        .unwrap();

    assert!(response.accepted);
    assert_eq!(response.payload, b"hello".to_vec());
    assert_eq!(
        response.provider_core_id.as_ref().map(|id| id.as_str()),
        Some("core-b")
    );
}

#[test]
fn owned_remote_invocation_transfers_the_payload_allocation() {
    let resolver = CapabilityResolver::new(CapabilityRegistry::new()).with_peers(vec![
        peer_with_rpc_endpoint("core-b", descriptor("runtime.echo"), false),
    ]);
    let mut request = request("runtime.echo");
    request.request_id = "req-owned".to_string();
    request.payload = vec![0x5a; 4 * 1_024 * 1_024];
    request.idempotency_key = Some("idem-owned".to_string());
    request.trace = Some(
        TraceContext::new(
            "trace-owned",
            "span-owned",
            CoreId::new("core-a").unwrap(),
            CoreId::new("core-a").unwrap(),
            TenantId::new("tenant-a").unwrap(),
        )
        .unwrap(),
    );
    let payload_pointer = request.payload.as_ptr();
    let invoker = PeerRpcRemoteCapabilityInvoker::new(OwnedPeerRpcClient {
        payload_pointer: payload_pointer as usize,
    });

    let response = resolver
        .handle_owned(
            &identity("core-a"),
            &service_id(),
            request,
            None,
            Some(&invoker),
            0,
        )
        .unwrap();

    assert!(response.accepted);
    assert_eq!(response.payload.as_ptr(), payload_pointer);
    assert_eq!(response.payload.len(), 4 * 1_024 * 1_024);
}

#[test]
fn remote_provider_without_peer_rpc_endpoint_is_unavailable() {
    let resolver = CapabilityResolver::new(CapabilityRegistry::new()).with_peers(vec![peer(
        "core-b",
        descriptor("runtime.echo"),
        false,
    )]);
    let invoker = PeerRpcRemoteCapabilityInvoker::new(FakePeerRpcClient);

    assert!(matches!(
        resolver.handle(
            &identity("core-a"),
            &service_id(),
            &request("runtime.echo"),
            None,
            Some(&invoker),
            0
        ),
        Err(CapabilityError::RemoteEndpointUnavailable(_))
    ));
}

#[test]
fn excludes_cross_tenant_and_cluster_local_visibility_peers() {
    let local = identity("core-a");
    let mut cross_tenant = peer("core-b", descriptor("runtime.echo"), false);
    cross_tenant.identity.tenant_id = TenantId::new("tenant-b").unwrap();
    let mut local_only = descriptor("runtime.local");
    local_only.visibility = CapabilityVisibility::Local;
    let resolver = CapabilityResolver::new(CapabilityRegistry::new())
        .with_peers(vec![cross_tenant, peer("core-c", local_only, false)]);

    assert!(matches!(
        resolver.resolve(&local, &service_id(), &request("runtime.echo"), None, 0),
        Err(CapabilityError::ProviderUnavailable(_))
    ));
    assert!(matches!(
        resolver.resolve(&local, &service_id(), &request("runtime.local"), None, 0),
        Err(CapabilityError::ProviderUnavailable(_))
    ));
}

#[test]
fn remote_disabled_policy_never_selects_remote() {
    let resolver = CapabilityResolver::new(CapabilityRegistry::new())
        .with_peers(vec![peer("core-b", descriptor("runtime.echo"), false)])
        .with_selector(Arc::new(DefaultCapabilitySelectionPolicy {
            policy: ResolutionPolicy {
                prefer_local: true,
                allow_remote: false,
            },
        }));

    assert!(matches!(
        resolver.resolve(
            &identity("core-a"),
            &service_id(),
            &request("runtime.echo"),
            None,
            0
        ),
        Err(CapabilityError::ProviderUnavailable(_))
    ));
}

#[test]
fn remote_leader_capability_checks_remote_holder() {
    let local = identity("core-a");
    let remote = identity("core-b");
    let mut write = descriptor("runtime.write");
    write.mode = CapabilityMode::Command;
    write.requirements.requires_leader = true;
    let resolver = CapabilityResolver::new(CapabilityRegistry::new())
        .with_peers(vec![peer("core-b", write, false)]);
    let service = service_id();
    let guard = StaticServiceLeadershipGuard::new([ServiceLeaderLease {
        service_id: service.clone(),
        tenant_id: local.tenant_id.clone(),
        cluster_id: local.cluster_id.clone(),
        holder_core_id: remote.core_id,
        epoch: 2,
        acquired_at_ms: 1,
        expires_at_ms: 100,
    }]);
    let mut command = request("runtime.write");
    command.mode = CapabilityMode::Command;

    let provider = resolver
        .resolve(&local, &service, &command, Some(&guard), 10)
        .unwrap();
    assert_eq!(provider.core_id().as_str(), "core-b");
}

#[test]
fn descriptor_catalog_rejects_duplicates_and_undeclared_requests() {
    let declared = descriptor("runtime.echo");
    let duplicate = CapabilityCatalog::from_descriptors([declared.clone(), declared]);
    assert!(matches!(
        duplicate,
        Err(CapabilityError::DescriptorAlreadyRegistered(_))
    ));

    let catalog = CapabilityCatalog::new();
    assert!(matches!(
        catalog.resolve_local(&request("runtime.echo")),
        Err(CapabilityError::CapabilityNotDeclared(_))
    ));
}

#[test]
fn descriptor_catalog_enforces_mode_idempotency_and_host_write_mode() {
    let core = identity("core-a");
    let service = service_id();
    let mut write = descriptor("runtime.write");
    write.mode = CapabilityMode::Command;
    write.requirements.idempotency_required = true;
    let catalog = CapabilityCatalog::from_descriptors([write]).unwrap();

    let mut command = request("runtime.write");
    command.mode = CapabilityMode::Command;
    let context = CapabilityEnforcementContext::new(&core, &service, 10);
    assert!(matches!(
        catalog.authorize_local(&command, context),
        Err(CapabilityError::HandlerRejected(reason)) if reason == "missing_idempotency_key"
    ));

    command.idempotency_key = Some("request-1".to_string());
    let read_only =
        CapabilityEnforcementContext::new(&core, &service, 10).with_writes_allowed(false);
    assert!(matches!(
        catalog.authorize_local(&command, read_only),
        Err(CapabilityError::WritesDisabled(_))
    ));

    command.mode = CapabilityMode::Query;
    let context = CapabilityEnforcementContext::new(&core, &service, 10);
    assert!(matches!(
        catalog.authorize_local(&command, context),
        Err(CapabilityError::HandlerRejected(reason)) if reason == "capability_mode_mismatch"
    ));
}
