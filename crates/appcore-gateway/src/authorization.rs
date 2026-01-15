// =============================================================================
//        #######
//     ###       ###     F: authorization.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/02 12:48:56 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:48:56 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Gateway connection authorization bindings.

use crate::{GatewayError, GatewayResult, GatewayState};
use appcore_contracts::InstallationId;
use appcore_security::{
    compute_request_hash, CommandTokenValidator, RequestValidationDetails, RuntimeTokenClaims,
    TokenClaims,
};
use appcore_types::{CapabilityName, ClusterId, CoreId, InstanceId, TenantId};

/// Maximum accepted lifetime for a one-use Gateway connection credential.
pub const GATEWAY_CONNECTION_TOKEN_TTL_MS: u64 = 60_000;

const GATEWAY_TOKEN_CLOCK_SKEW_MS: u64 = 5_000;

/// Returns the outer claims required for Gateway connection credentials.
pub fn gateway_token_claims() -> TokenClaims {
    TokenClaims {
        issuer: "appcore".to_string(),
        audience: "gateway".to_string(),
        salt: "peer".to_string(),
        ttl_ms: GATEWAY_CONNECTION_TOKEN_TTL_MS,
    }
}

/// Computes the request hash that binds a worker credential to one connection identity.
pub fn worker_connection_hash(
    tenant_id: &TenantId,
    cluster_id: &ClusterId,
    installation_id: &InstallationId,
    core_id: &CoreId,
    capabilities: &[CapabilityName],
) -> String {
    let mut capabilities = capabilities
        .iter()
        .map(|capability| capability.as_str())
        .collect::<Vec<_>>();
    capabilities.sort_unstable();
    capabilities.dedup();
    connection_hash(
        "gateway.worker.v2",
        tenant_id.as_str(),
        Some(installation_id.as_str()),
        &encode_capabilities(&capabilities),
        Some(core_id.as_str()),
        Some(cluster_id.as_str()),
    )
}

/// Computes the request hash that binds a client credential to tenant, cluster and device.
pub fn client_connection_hash(
    tenant_id: &TenantId,
    cluster_id: &ClusterId,
    device_id: &InstanceId,
) -> String {
    connection_hash(
        "gateway.client.v2",
        tenant_id.as_str(),
        Some(device_id.as_str()),
        "",
        None,
        Some(cluster_id.as_str()),
    )
}

pub(crate) fn authenticate_connection(
    state: &GatewayState,
    token: &str,
    expected_hash: &str,
    now_ms: u64,
) -> GatewayResult<RuntimeTokenClaims> {
    let claims = CommandTokenValidator::new(&state.token_provider, gateway_token_claims())
        .validate_and_get_claims(token, "peer", None, now_ms, Some(expected_hash))
        .map_err(|_| GatewayError::Authentication("connection token is invalid".to_string()))?;
    let jti = claims
        .jti
        .as_deref()
        .ok_or_else(|| GatewayError::Authentication("connection token has no replay id".into()))?;
    if claims.request_hash.as_deref() != Some(expected_hash)
        || claims.issued_at_ms > now_ms.saturating_add(GATEWAY_TOKEN_CLOCK_SKEW_MS)
        || claims.expires_at_ms <= claims.issued_at_ms
        || claims.expires_at_ms.saturating_sub(claims.issued_at_ms)
            > GATEWAY_CONNECTION_TOKEN_TTL_MS
    {
        return Err(GatewayError::Authentication(
            "connection token claims are invalid".to_string(),
        ));
    }
    state
        .connection_replay()
        .check_and_record(jti, claims.expires_at_ms, now_ms)
        .map_err(|_| GatewayError::Authentication("connection token replay rejected".into()))?;
    Ok(claims)
}

pub(crate) fn authenticate_mesh_request(
    state: &GatewayState,
    token: &str,
    request: &crate::MeshPeerRequest,
    now_ms: u64,
) -> GatewayResult<()> {
    let expected_hash = request
        .expected_request_hash()
        .map_err(|_| GatewayError::Protocol("mesh request metadata is invalid".to_string()))?;
    let claims = CommandTokenValidator::new(&state.token_provider, gateway_token_claims())
        .validate_and_get_claims(token, "peer", None, now_ms, expected_hash.as_deref())
        .map_err(|_| GatewayError::Authentication("mesh token is invalid".to_string()))?;
    if expected_hash.is_some() && claims.request_hash.as_deref() != expected_hash.as_deref() {
        return Err(GatewayError::Forbidden(
            "mesh token is not bound to the request".to_string(),
        ));
    }
    if request.bearer_token.as_deref() != Some(token) {
        return Err(GatewayError::Authentication(
            "forwarded mesh credential mismatch".to_string(),
        ));
    }
    Ok(())
}

fn connection_hash(
    name: &str,
    tenant_id: &str,
    identity: Option<&str>,
    payload: &str,
    subject: Option<&str>,
    audience: Option<&str>,
) -> String {
    compute_request_hash(&RequestValidationDetails {
        purpose: "peer".to_string(),
        name: name.to_string(),
        id: tenant_id.to_string(),
        idempotency_key: identity.map(ToOwned::to_owned),
        payload: payload.to_string(),
        subject: subject.map(ToOwned::to_owned),
        audience: audience.map(ToOwned::to_owned),
    })
}

fn encode_capabilities(capabilities: &[&str]) -> String {
    let capacity = capabilities
        .iter()
        .map(|capability| capability.len().saturating_add(8))
        .sum::<usize>();
    let mut framed = Vec::with_capacity(capacity);
    for capability in capabilities {
        framed.extend_from_slice(&(capability.len() as u64).to_be_bytes());
        framed.extend_from_slice(capability.as_bytes());
    }
    let mut encoded = String::with_capacity(framed.len() * 2);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in framed {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GatewayConfig, MeshPeerRequest};
    use appcore_distributed_contracts::PeerRpcEnvelope;
    use appcore_peer_rpc::{
        envelope_signing_hash, BoundedReplayStore, PeerNonceStore, PeerRpcHttpRequest,
        ReplayStoreConfig, PEER_QUERY_PATH,
    };
    use appcore_security::{CommandTokenFactory, HashTokenProvider};
    use appcore_types::{ClusterId, CoreId};
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn state() -> Arc<GatewayState> {
        state_with_replay(Arc::new(BoundedReplayStore::new(
            ReplayStoreConfig::default(),
        )))
    }

    fn state_with_replay(replay: Arc<dyn PeerNonceStore>) -> Arc<GatewayState> {
        let provider = HashTokenProvider::from_secret(vec![7; 32]).unwrap();
        Arc::new(
            GatewayState::with_replay_store(
                GatewayConfig::new(([127, 0, 0, 1], 8080).into(), "gateway.test"),
                provider,
                replay,
            )
            .unwrap(),
        )
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    fn connection_token(
        state: &GatewayState,
        hash: &str,
        jti: Option<&str>,
        issued_at_ms: u64,
    ) -> String {
        CommandTokenFactory::new(&state.token_provider, gateway_token_claims())
            .create_v1_with_jti_and_hash(
                "peer",
                None,
                None,
                None,
                issued_at_ms,
                30_000,
                jti.map(ToOwned::to_owned),
                Some(hash.to_string()),
            )
            .unwrap()
    }

    #[test]
    fn connection_token_is_bound_and_single_use() {
        let state = state();
        let now = now_ms();
        let hash = client_connection_hash(
            &TenantId::new("tenant-a").unwrap(),
            &ClusterId::new("cluster-a").unwrap(),
            &InstanceId::new("device-a").unwrap(),
        );
        let token = connection_token(&state, &hash, Some("connection-jti-a"), now);

        assert!(authenticate_connection(&state, &token, &hash, now).is_ok());
        assert!(authenticate_connection(&state, &token, &hash, now).is_err());
    }

    #[test]
    fn shared_replay_store_rejects_a_token_across_gateway_states() {
        let replay: Arc<dyn PeerNonceStore> =
            Arc::new(BoundedReplayStore::new(ReplayStoreConfig::default()));
        let first = state_with_replay(Arc::clone(&replay));
        let second = state_with_replay(replay);
        let now = now_ms();
        let hash = client_connection_hash(
            &TenantId::new("tenant-shared").unwrap(),
            &ClusterId::new("cluster-shared").unwrap(),
            &InstanceId::new("device-shared").unwrap(),
        );
        let token = connection_token(&first, &hash, Some("shared-replay-jti"), now);

        assert!(authenticate_connection(&first, &token, &hash, now).is_ok());
        assert!(authenticate_connection(&second, &token, &hash, now).is_err());
    }

    #[test]
    fn connection_token_rejects_wrong_binding_missing_jti_and_future_issue() {
        let state = state();
        let now = now_ms();
        let token = connection_token(&state, "expected", Some("jti-wrong"), now);
        assert!(authenticate_connection(&state, &token, "different", now).is_err());

        let token = connection_token(&state, "expected", None, now);
        assert!(authenticate_connection(&state, &token, "expected", now).is_err());

        let token = connection_token(&state, "expected", Some("jti-future"), now + 6_000);
        assert!(authenticate_connection(&state, &token, "expected", now).is_err());

        let token = CommandTokenFactory::new(&state.token_provider, gateway_token_claims())
            .create_v1_with_jti_and_hash(
                "peer",
                None,
                None,
                None,
                now - 60_000,
                GATEWAY_CONNECTION_TOKEN_TTL_MS + 30_000,
                Some("jti-long-lived".to_string()),
                Some("expected".to_string()),
            )
            .unwrap();
        assert!(authenticate_connection(&state, &token, "expected", now).is_err());
    }

    #[test]
    fn mesh_token_must_bind_inner_envelope_and_forward_the_same_credential() {
        let state = state();
        let now = now_ms();
        let envelope = PeerRpcEnvelope::new(
            "request-a",
            "trace-a",
            CoreId::new("core-source").unwrap(),
            CoreId::new("core-target").unwrap(),
            TenantId::new("tenant-a").unwrap(),
            ClusterId::new("cluster-a").unwrap(),
            now,
            now + 30_000,
            "nonce-a",
            CapabilityName::new("runtime.query").unwrap(),
            b"opaque".to_vec(),
            None,
            None,
        );
        let hash = envelope_signing_hash(&envelope);
        let token = connection_token(&state, &hash, Some("mesh-jti-a"), now);
        let mut request = MeshPeerRequest::new(
            "request-a",
            envelope.tenant_id.clone(),
            envelope.target_core_id.clone(),
            PeerRpcHttpRequest {
                method: "POST".to_string(),
                path: PEER_QUERY_PATH.to_string(),
                body: serde_json::to_vec(&envelope).unwrap(),
                bearer_token: Some(token.clone()),
                timeout_ms: 1_000,
                max_response_bytes: 4_096,
            },
        );
        assert!(authenticate_mesh_request(&state, &token, &request, now).is_ok());

        request.target_tenant_id = TenantId::new("tenant-b").unwrap();
        assert!(authenticate_mesh_request(&state, &token, &request, now).is_err());
        request.target_tenant_id = TenantId::new("tenant-a").unwrap();
        request.bearer_token = Some("different-token".to_string());
        assert!(authenticate_mesh_request(&state, &token, &request, now).is_err());
    }
}
