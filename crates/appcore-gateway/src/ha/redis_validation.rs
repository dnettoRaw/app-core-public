// =============================================================================
//        #######
//     ###       ###     F: redis_validation.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================

//! Local validation and serialization helpers for Redis operations.

use super::{
    GatewayInstanceLease, GatewayRegistryError, GatewayRegistryResult, GatewayRequestFence,
};

pub(crate) fn bounded_expiry(now_ms: u64, ttl_ms: u64, maximum: u64) -> GatewayRegistryResult<u64> {
    if ttl_ms == 0 || ttl_ms > maximum {
        return Err(GatewayRegistryError::InvalidContract);
    }
    now_ms
        .checked_add(ttl_ms)
        .ok_or(GatewayRegistryError::InvalidContract)
}

pub(crate) fn absolute_ttl(
    expires_at_ms: u64,
    now_ms: u64,
    maximum: u64,
) -> GatewayRegistryResult<u64> {
    expires_at_ms
        .checked_sub(now_ms)
        .filter(|ttl| *ttl > 0 && *ttl <= maximum)
        .ok_or(GatewayRegistryError::InvalidContract)
}

pub(crate) fn ensure_live(lease: &GatewayInstanceLease, now_ms: u64) -> GatewayRegistryResult<()> {
    if lease.is_expired(now_ms) {
        return Err(GatewayRegistryError::Expired);
    }
    Ok(())
}

pub(crate) fn ensure_same_owner(
    current: &GatewayInstanceLease,
    recorded: &GatewayInstanceLease,
) -> GatewayRegistryResult<()> {
    if current.tenant_id() != recorded.tenant_id()
        || current.cluster_id() != recorded.cluster_id()
        || current.instance_id() != recorded.instance_id()
        || current.epoch() != recorded.epoch()
    {
        return Err(GatewayRegistryError::StaleOwner);
    }
    Ok(())
}

pub(crate) fn validate_request(
    request: &GatewayRequestFence,
    now_ms: u64,
) -> GatewayRegistryResult<()> {
    validate_request_shape(request)?;
    if request.is_expired(now_ms) {
        return Err(GatewayRegistryError::Expired);
    }
    Ok(())
}

pub(crate) fn validate_request_shape(request: &GatewayRequestFence) -> GatewayRegistryResult<()> {
    if request.request_id.is_empty()
        || request.request_id.len() > 128
        || !request
            .request_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
        || request.origin_epoch == 0
        || request.target_epoch == 0
        || request.worker_generation == 0
        || request.expires_at_ms == 0
    {
        return Err(GatewayRegistryError::InvalidContract);
    }
    Ok(())
}

pub(crate) fn encode<T: serde::Serialize>(value: &T) -> GatewayRegistryResult<String> {
    serde_json::to_string(value).map_err(|_| GatewayRegistryError::InvalidContract)
}

pub(crate) fn decode<T: serde::de::DeserializeOwned>(value: &str) -> GatewayRegistryResult<T> {
    serde_json::from_str(value).map_err(|_| GatewayRegistryError::InvalidContract)
}

pub(crate) fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}
