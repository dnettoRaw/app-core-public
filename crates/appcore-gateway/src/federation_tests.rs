// =============================================================================
//        #######
//     ###       ###     F: federation_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.4-rc
// =============================================================================
// appcore-norm: test

use super::*;
use crate::federation_auth::{authenticate_federation_request, mint_federation_token};
use crate::{
    GatewayConfig, GatewayFederationUrl, GatewayInstanceLease, GatewayState, GatewayWorkerRecord,
    GatewayWorkerRegistration,
};
use appcore_contracts::InstallationId;
use appcore_distributed_contracts::PeerRpcEnvelope;
use appcore_peer_rpc::{
    v2::{PeerRpcWireErrorCodeV2, PeerRpcWireErrorV2},
    PeerRpcHttpRequest, PeerRpcHttpResponse, PEER_QUERY_PATH,
};
use appcore_security::{CommandTokenFactory, HashTokenProvider};
use appcore_types::{CapabilityName, ClusterId, CoreId, InstanceId, TenantId};

const NOW_MS: u64 = 1_000;

#[test]
fn federation_contract_binds_fence_inner_request_and_typed_response() {
    let request = request();
    assert!(request.validate().is_ok());
    let original_hash = request.body_hash().unwrap();
    let mut changed = request.clone();
    changed.fence.worker_generation += 1;
    assert_ne!(changed.body_hash().unwrap(), original_hash);

    let success = GatewayFederationResponseV2::ok(
        request.fence.clone(),
        MeshPeerResponse::ok(
            request.request.request_id.clone(),
            PeerRpcHttpResponse {
                status_code: 200,
                body: b"bounded".to_vec(),
            },
        ),
    );
    assert!(success.validate_for_request(&request).is_ok());

    let rejection = GatewayFederationResponseV2::rejected(
        request.fence.clone(),
        PeerRpcWireErrorV2::controlled(
            Some(request.request.request_id.clone()),
            None,
            PeerRpcWireErrorCodeV2::EndpointUnavailable,
        ),
    );
    assert!(rejection.validate_for_request(&request).is_ok());
    let inner_credential = request.request.bearer_token.as_deref().unwrap();
    assert!(!format!("{request:?}").contains(inner_credential));
}

#[test]
fn federation_credential_is_body_bound_short_lived_and_single_use() {
    let origin = state();
    let target = state();
    let request = request();
    let token = mint_federation_token(&origin, &request, NOW_MS).unwrap();

    let mut changed = request.clone();
    changed.fence.target_epoch += 1;
    assert!(authenticate_federation_request(&target, &token, &changed, NOW_MS).is_err());
    assert!(authenticate_federation_request(&target, &token, &request, NOW_MS).is_ok());
    assert!(authenticate_federation_request(&target, &token, &request, NOW_MS).is_err());
}

#[test]
fn federation_rejects_same_owner_and_mismatched_identity() {
    let mut same_owner = request();
    same_owner.fence.target_instance_id = same_owner.fence.origin_instance_id.clone();
    assert!(same_owner.validate().is_err());

    let mut mismatched = request();
    mismatched.fence.target_core_id = CoreId::new("different-core").unwrap();
    assert!(mismatched.validate().is_err());
}

fn state() -> GatewayState {
    GatewayState::new(
        GatewayConfig::new(([127, 0, 0, 1], 8080).into(), "gateway.test"),
        HashTokenProvider::from_secret(vec![29; 32]).unwrap(),
    )
    .unwrap()
}

fn request() -> GatewayFederationRequestV2 {
    let tenant = TenantId::new("tenant-a").unwrap();
    let cluster = ClusterId::new("cluster-a").unwrap();
    let source_core = CoreId::new("source-core").unwrap();
    let target_core = CoreId::new("target-core").unwrap();
    let origin = GatewayInstanceLease::new(
        tenant.clone(),
        cluster.clone(),
        InstanceId::new("gateway-origin").unwrap(),
        GatewayFederationUrl::new("https://origin.example.test").unwrap(),
        3,
        60_000,
    )
    .unwrap();
    let target = GatewayInstanceLease::new(
        tenant.clone(),
        cluster.clone(),
        InstanceId::new("gateway-target").unwrap(),
        GatewayFederationUrl::new("https://target.example.test").unwrap(),
        7,
        60_000,
    )
    .unwrap();
    let worker = GatewayWorkerRecord::new(
        target,
        GatewayWorkerRegistration::new(
            InstallationId::new("install-target").unwrap(),
            target_core.clone(),
            11,
            vec![CapabilityName::new("runtime.query").unwrap()],
        )
        .unwrap(),
        50_000,
    )
    .unwrap();
    let envelope = PeerRpcEnvelope::new(
        "request-a",
        "trace-a",
        source_core,
        target_core.clone(),
        tenant.clone(),
        cluster,
        NOW_MS,
        20_000,
        "inner-nonce",
        CapabilityName::new("runtime.query").unwrap(),
        b"opaque".to_vec(),
        None,
        None,
    );
    let inner_hash = appcore_peer_rpc::envelope_signing_hash(&envelope);
    let inner_credential =
        CommandTokenFactory::new(&state().token_provider, crate::gateway_token_claims())
            .create_v1_with_jti_and_hash(
                "peer",
                None,
                None,
                None,
                NOW_MS,
                5_000,
                Some("inner-federation-test".to_string()),
                Some(inner_hash),
            )
            .unwrap();
    let mesh = MeshPeerRequest::new(
        "request-a",
        tenant,
        target_core,
        PeerRpcHttpRequest {
            method: "POST".to_string(),
            path: PEER_QUERY_PATH.to_string(),
            body: serde_json::to_vec(&envelope).unwrap(),
            bearer_token: Some(inner_credential),
            timeout_ms: 5_000,
            max_response_bytes: 4_096,
        },
    );
    let fence = GatewayRequestFence::new(&origin, &worker, "request-a", 20_000).unwrap();
    GatewayFederationRequestV2::new(fence, mesh).unwrap()
}
