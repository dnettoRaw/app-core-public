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
    RuntimeContractVersion, RuntimeIdentity, SyncGroup, TenantId,
};
use appcore_peer_rpc::{
    PeerRpcCallKind, PeerRpcClientExecutor, PeerRpcError, PeerRpcOutboundRequest, PeerRpcResponse,
};
use std::collections::BTreeMap;
use std::sync::Arc;

struct EchoHandler {
    descriptor: CapabilityDescriptor,
}

struct FakePeerRpcClient;

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
