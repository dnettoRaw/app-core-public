// =============================================================================
//        #######
//     ###       ###     F: redis_config.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.6-rc
// =============================================================================

//! Bounded Redis registry configuration without embedded credentials.

use super::{GatewayRegistryError, GatewayRegistryResult};
use std::fmt::{Debug, Formatter};
use std::time::Duration;
use zeroize::Zeroizing;

/// Maximum Redis registry namespace length.
pub const MAX_GATEWAY_REDIS_NAMESPACE_BYTES: usize = 64;
/// Maximum simultaneous Redis registry operations.
pub const MAX_GATEWAY_REGISTRY_CONCURRENCY: usize = 64;
/// Maximum accepted instance or worker ownership TTL.
pub const MAX_GATEWAY_INSTANCE_LEASE_TTL_MS: u64 = 60_000;
/// Maximum number of workers returned by one shared resolution.
pub const MAX_GATEWAY_RESOLVE_CANDIDATES: usize = 1_024;

/// Zeroizing Redis credential supplied by the deployment composition root.
pub struct RedisGatewayCredential(Zeroizing<String>);

impl RedisGatewayCredential {
    /// Takes ownership of non-empty resolved secret material without copying it.
    pub fn new(value: Zeroizing<String>) -> GatewayRegistryResult<Self> {
        if value.trim().is_empty() {
            return Err(GatewayRegistryError::InvalidContract);
        }
        Ok(Self(value))
    }

    /// Borrows the credential only for immediate Redis authentication.
    pub(crate) fn expose(&self) -> &str {
        self.0.as_str()
    }
}

impl Debug for RedisGatewayCredential {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("RedisGatewayCredential(REDACTED)")
    }
}

/// Validated opt-in Redis HA registry configuration.
#[derive(Clone, PartialEq, Eq)]
pub struct RedisGatewayRegistryConfig {
    endpoint: String,
    namespace: String,
    operation_timeout: Duration,
    max_concurrency: usize,
}

impl RedisGatewayRegistryConfig {
    /// Creates a bounded configuration with no credentials in the endpoint.
    pub fn new(
        endpoint: impl Into<String>,
        namespace: impl Into<String>,
        operation_timeout_ms: u64,
        max_concurrency: usize,
    ) -> GatewayRegistryResult<Self> {
        let endpoint = endpoint.into();
        let namespace = namespace.into();
        if !valid_redis_endpoint(&endpoint)
            || !valid_namespace(&namespace)
            || operation_timeout_ms == 0
            || operation_timeout_ms > super::MAX_GATEWAY_REGISTRY_OPERATION_TIMEOUT_MS
            || max_concurrency == 0
            || max_concurrency > MAX_GATEWAY_REGISTRY_CONCURRENCY
        {
            return Err(GatewayRegistryError::InvalidContract);
        }
        Ok(Self {
            endpoint,
            namespace,
            operation_timeout: Duration::from_millis(operation_timeout_ms),
            max_concurrency,
        })
    }

    /// Borrows the endpoint only for immediate Redis client construction.
    pub(crate) fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Returns the validated key namespace.
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// Returns the command and connection timeout.
    pub const fn operation_timeout(&self) -> Duration {
        self.operation_timeout
    }

    /// Returns the registry concurrency ceiling.
    pub const fn max_concurrency(&self) -> usize {
        self.max_concurrency
    }
}

impl Debug for RedisGatewayRegistryConfig {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RedisGatewayRegistryConfig")
            .field("endpoint", &"REDACTED")
            .field("namespace", &self.namespace)
            .field("operation_timeout", &self.operation_timeout)
            .field("max_concurrency", &self.max_concurrency)
            .finish()
    }
}

fn valid_namespace(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_GATEWAY_REDIS_NAMESPACE_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn valid_redis_endpoint(value: &str) -> bool {
    if value.is_empty()
        || value.len() > super::MAX_GATEWAY_FEDERATION_URL_BYTES
        || value != value.trim()
        || value.contains(['@', '?', '#'])
    {
        return false;
    }
    if let Some(target) = value.strip_prefix("rediss://") {
        return valid_target(target, false);
    }
    value
        .strip_prefix("redis://")
        .is_some_and(|target| valid_target(target, true))
}

fn valid_target(value: &str, loopback_only: bool) -> bool {
    let (authority, database) = value.split_once('/').unwrap_or((value, ""));
    if authority.is_empty()
        || (!database.is_empty() && !database.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return false;
    }
    if !loopback_only {
        return true;
    }
    authority == "127.0.0.1"
        || authority == "localhost"
        || authority == "[::1]"
        || authority.starts_with("127.0.0.1:")
        || authority.starts_with("localhost:")
        || authority.starts_with("[::1]:")
}

#[cfg(test)]
#[path = "redis_config_tests.rs"]
mod tests;
