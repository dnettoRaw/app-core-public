// =============================================================================
//        #######
//     ###       ###     F: federation_auth.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.4-rc
// =============================================================================

//! One-use outer authentication for Gateway federation V2.

use crate::{GatewayError, GatewayFederationRequestV2, GatewayResult, GatewayState};
use appcore_security::{CommandTokenFactory, CommandTokenValidator, TokenClaims};

/// Maximum lifetime accepted for a Gateway federation credential.
pub const GATEWAY_FEDERATION_TOKEN_TTL_MS: u64 = 30_000;

const CLOCK_SKEW_MS: u64 = 5_000;

/// Returns the isolated cryptographic claims domain for federation V2.
pub fn gateway_federation_token_claims() -> TokenClaims {
    TokenClaims {
        issuer: "appcore".to_string(),
        audience: "gateway-federation-v2".to_string(),
        salt: "gateway-federation-v2".to_string(),
        ttl_ms: GATEWAY_FEDERATION_TOKEN_TTL_MS,
    }
}

pub(crate) fn mint_federation_token(
    state: &GatewayState,
    request: &GatewayFederationRequestV2,
    now_ms: u64,
) -> GatewayResult<String> {
    let request_hash = request.body_hash()?;
    let ttl_ms = request
        .fence
        .expires_at_ms
        .saturating_sub(now_ms)
        .min(GATEWAY_FEDERATION_TOKEN_TTL_MS);
    if ttl_ms == 0 {
        return Err(authentication_error("federation request expired"));
    }
    CommandTokenFactory::new(&state.token_provider, gateway_federation_token_claims())
        .create_v1_with_jti_and_hash(
            "peer",
            None,
            None,
            Some(request.fence.origin_instance_id.as_str()),
            now_ms,
            ttl_ms,
            Some(federation_jti(&request_hash)),
            Some(request_hash),
        )
        .map_err(|_| authentication_error("federation credential issuance failed"))
}

pub(crate) fn authenticate_federation_request(
    state: &GatewayState,
    token: &str,
    request: &GatewayFederationRequestV2,
    now_ms: u64,
) -> GatewayResult<()> {
    let request_hash = request.body_hash()?;
    let claims =
        CommandTokenValidator::new(&state.token_provider, gateway_federation_token_claims())
            .validate_and_get_claims(token, "peer", None, now_ms, Some(&request_hash))
            .map_err(|_| authentication_error("federation credential is invalid"))?;
    let expected_jti = federation_jti(&request_hash);
    if claims.request_hash.as_deref() != Some(request_hash.as_str())
        || claims.jti.as_deref() != Some(expected_jti.as_str())
        || claims.subject.as_deref() != Some(request.fence.origin_instance_id.as_str())
        || claims.issued_at_ms > now_ms.saturating_add(CLOCK_SKEW_MS)
        || claims.expires_at_ms <= claims.issued_at_ms
        || claims.expires_at_ms > request.fence.expires_at_ms
        || claims.expires_at_ms.saturating_sub(claims.issued_at_ms)
            > GATEWAY_FEDERATION_TOKEN_TTL_MS
    {
        return Err(authentication_error(
            "federation credential claims are invalid",
        ));
    }
    state
        .connection_replay()
        .check_and_record(&expected_jti, claims.expires_at_ms, now_ms)
        .map_err(|_| authentication_error("federation credential replay rejected"))?;
    Ok(())
}

fn federation_jti(request_hash: &str) -> String {
    format!("gateway-federation-v2:{request_hash}")
}

fn authentication_error(message: &'static str) -> GatewayError {
    GatewayError::Authentication(message.to_string())
}
