// =============================================================================
//        #######
//     ###       ###     F: types.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================

//! Validated identities and records shared by Gateway HA providers.

use crate::config::MAX_GATEWAY_CAPABILITIES;
use appcore_contracts::InstallationId;
use appcore_types::{CapabilityName, ClusterId, CoreId, InstanceId, TenantId};
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Formatter};
/// Stable stored schema for the opt-in Gateway HA contract.
pub const GATEWAY_HA_SCHEMA_V2: &str = "appcore.gateway.ha.v2";
/// Maximum accepted federation base URL size.
pub const MAX_GATEWAY_FEDERATION_URL_BYTES: usize = 2_048;
/// Maximum configured provider operation timeout.
pub const MAX_GATEWAY_REGISTRY_OPERATION_TIMEOUT_MS: u64 = 5_000;
/// Validated federation base URL; debug output always redacts it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct GatewayFederationUrl(String);

impl GatewayFederationUrl {
    /// Validates an HTTPS URL or loopback-only HTTP URL without credentials.
    pub fn new(value: impl Into<String>) -> GatewayRegistryResult<Self> {
        let value = value.into();
        if !valid_federation_url(&value) {
            return Err(GatewayRegistryError::InvalidContract);
        }
        Ok(Self(value))
    }

    /// Borrows the URL for immediate bounded transport construction.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl Debug for GatewayFederationUrl {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("GatewayFederationUrl(REDACTED)")
    }
}

impl TryFrom<String> for GatewayFederationUrl {
    type Error = GatewayRegistryError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<GatewayFederationUrl> for String {
    fn from(value: GatewayFederationUrl) -> Self {
        value.0
    }
}

/// Tenant-local lease held by one live Gateway instance.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayInstanceLease {
    schema: String,
    tenant_id: TenantId,
    cluster_id: ClusterId,
    instance_id: InstanceId,
    federation_url: GatewayFederationUrl,
    epoch: u64,
    expires_at_ms: u64,
}

impl GatewayInstanceLease {
    /// Creates one validated monotonic instance lease.
    pub fn new(
        tenant_id: TenantId,
        cluster_id: ClusterId,
        instance_id: InstanceId,
        federation_url: GatewayFederationUrl,
        epoch: u64,
        expires_at_ms: u64,
    ) -> GatewayRegistryResult<Self> {
        if epoch == 0 || expires_at_ms == 0 {
            return Err(GatewayRegistryError::InvalidContract);
        }
        Ok(Self {
            schema: GATEWAY_HA_SCHEMA_V2.to_string(),
            tenant_id,
            cluster_id,
            instance_id,
            federation_url,
            epoch,
            expires_at_ms,
        })
    }

    /// Validates a record decoded from shared storage.
    pub fn validate(&self) -> GatewayRegistryResult<()> {
        if self.schema != GATEWAY_HA_SCHEMA_V2 {
            return Err(GatewayRegistryError::UnsupportedSchema);
        }
        if self.epoch == 0 || self.expires_at_ms == 0 {
            return Err(GatewayRegistryError::InvalidContract);
        }
        Ok(())
    }

    /// Returns the tenant isolation boundary.
    pub fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }

    /// Returns the cluster isolation boundary.
    pub fn cluster_id(&self) -> &ClusterId {
        &self.cluster_id
    }

    /// Returns the stable Gateway instance identity.
    pub fn instance_id(&self) -> &InstanceId {
        &self.instance_id
    }

    /// Returns the redacted federation URL owner.
    pub fn federation_url(&self) -> &GatewayFederationUrl {
        &self.federation_url
    }

    /// Returns the monotonic fencing epoch.
    pub const fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Returns the absolute lease expiry.
    pub const fn expires_at_ms(&self) -> u64 {
        self.expires_at_ms
    }

    /// Reports whether the lease expired at `now_ms`.
    pub const fn is_expired(&self, now_ms: u64) -> bool {
        now_ms >= self.expires_at_ms
    }
}

impl Debug for GatewayInstanceLease {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GatewayInstanceLease")
            .field("tenant_id", &self.tenant_id)
            .field("cluster_id", &self.cluster_id)
            .field("instance_id", &self.instance_id)
            .field("epoch", &self.epoch)
            .field("expires_at_ms", &self.expires_at_ms)
            .finish()
    }
}

/// Local worker data supplied while registering shared ownership.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayWorkerRegistration {
    /// Worker installation identity.
    pub installation_id: InstallationId,
    /// Worker Core identity.
    pub core_id: CoreId,
    /// Process-local connection generation protected by the instance epoch.
    pub generation: u64,
    /// Bounded advertised capabilities.
    pub capabilities: Vec<CapabilityName>,
}

impl GatewayWorkerRegistration {
    /// Creates a bounded registration and removes duplicate capabilities.
    pub fn new(
        installation_id: InstallationId,
        core_id: CoreId,
        generation: u64,
        mut capabilities: Vec<CapabilityName>,
    ) -> GatewayRegistryResult<Self> {
        capabilities.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        capabilities.dedup_by(|left, right| left.as_str() == right.as_str());
        if generation == 0 || capabilities.len() > MAX_GATEWAY_CAPABILITIES {
            return Err(GatewayRegistryError::InvalidContract);
        }
        Ok(Self {
            installation_id,
            core_id,
            generation,
            capabilities,
        })
    }

    /// Validates a registration assembled outside the constructor.
    pub fn validate(&self) -> GatewayRegistryResult<()> {
        if self.generation == 0 || self.capabilities.len() > MAX_GATEWAY_CAPABILITIES {
            return Err(GatewayRegistryError::InvalidContract);
        }
        if self
            .capabilities
            .windows(2)
            .any(|pair| pair[0].as_str() >= pair[1].as_str())
        {
            return Err(GatewayRegistryError::InvalidContract);
        }
        Ok(())
    }
}

/// Shared live worker ownership record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayWorkerRecord {
    /// Exact owner instance lease.
    pub owner: GatewayInstanceLease,
    /// Worker installation identity.
    pub installation_id: InstallationId,
    /// Worker Core identity.
    pub core_id: CoreId,
    /// Process-local connection generation.
    pub generation: u64,
    /// Bounded advertised capabilities.
    pub capabilities: Vec<CapabilityName>,
    /// Absolute worker record expiry.
    pub expires_at_ms: u64,
}

impl GatewayWorkerRecord {
    /// Creates a shared worker record under one owner fence.
    pub fn new(
        owner: GatewayInstanceLease,
        registration: GatewayWorkerRegistration,
        expires_at_ms: u64,
    ) -> GatewayRegistryResult<Self> {
        registration.validate()?;
        if expires_at_ms == 0 || expires_at_ms > owner.expires_at_ms() {
            return Err(GatewayRegistryError::InvalidContract);
        }
        Ok(Self {
            owner,
            installation_id: registration.installation_id,
            core_id: registration.core_id,
            generation: registration.generation,
            capabilities: registration.capabilities,
            expires_at_ms,
        })
    }

    /// Validates a worker record decoded from shared storage.
    pub fn validate(&self) -> GatewayRegistryResult<()> {
        self.owner.validate()?;
        if self.generation == 0
            || self.expires_at_ms == 0
            || self.expires_at_ms > self.owner.expires_at_ms()
            || self.capabilities.len() > MAX_GATEWAY_CAPABILITIES
            || self
                .capabilities
                .windows(2)
                .any(|pair| pair[0].as_str() >= pair[1].as_str())
        {
            return Err(GatewayRegistryError::InvalidContract);
        }
        Ok(())
    }

    /// Reports whether this worker or its owner lease is expired.
    pub const fn is_expired(&self, now_ms: u64) -> bool {
        now_ms >= self.expires_at_ms || self.owner.is_expired(now_ms)
    }
}

/// Shared authenticated client-session ownership record.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewaySessionRecord {
    /// Exact owner instance lease.
    pub owner: GatewayInstanceLease,
    /// Bounded session identity.
    pub session_id: String,
    /// Absolute authenticated session expiry.
    pub expires_at_ms: u64,
}

impl Debug for GatewaySessionRecord {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GatewaySessionRecord")
            .field("owner", &self.owner)
            .field("has_session_id", &!self.session_id.is_empty())
            .field("expires_at_ms", &self.expires_at_ms)
            .finish()
    }
}

impl GatewaySessionRecord {
    /// Creates a session record under one owner fence.
    pub fn new(
        owner: GatewayInstanceLease,
        session_id: impl Into<String>,
        expires_at_ms: u64,
    ) -> GatewayRegistryResult<Self> {
        let session_id = session_id.into();
        if !valid_identifier(&session_id) || expires_at_ms == 0 {
            return Err(GatewayRegistryError::InvalidContract);
        }
        Ok(Self {
            owner,
            session_id,
            expires_at_ms,
        })
    }

    /// Validates a session record decoded from shared storage.
    pub fn validate(&self) -> GatewayRegistryResult<()> {
        self.owner.validate()?;
        if !valid_identifier(&self.session_id) || self.expires_at_ms == 0 {
            return Err(GatewayRegistryError::InvalidContract);
        }
        Ok(())
    }
}

/// Exact origin/target fencing record for one in-flight request.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRequestFence {
    /// Tenant boundary copied from both owners.
    pub tenant_id: TenantId,
    /// Bounded request identity.
    pub request_id: String,
    /// Origin instance identity.
    pub origin_instance_id: InstanceId,
    /// Origin instance epoch.
    pub origin_epoch: u64,
    /// Target instance identity.
    pub target_instance_id: InstanceId,
    /// Cluster containing the selected target worker.
    pub target_cluster_id: ClusterId,
    /// Core identity of the selected target worker.
    pub target_core_id: CoreId,
    /// Target instance epoch.
    pub target_epoch: u64,
    /// Target worker connection generation.
    pub worker_generation: u64,
    /// Absolute request expiry.
    pub expires_at_ms: u64,
}

impl Debug for GatewayRequestFence {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GatewayRequestFence")
            .field("tenant_id", &self.tenant_id)
            .field("has_request_id", &!self.request_id.is_empty())
            .field("origin_epoch", &self.origin_epoch)
            .field("target_epoch", &self.target_epoch)
            .field("worker_generation", &self.worker_generation)
            .field("expires_at_ms", &self.expires_at_ms)
            .finish()
    }
}

impl GatewayRequestFence {
    /// Creates a request fence after checking origin/target tenant and identity.
    ///
    /// Request expiry may cross a lease-renewal boundary. Providers still check
    /// both exact owner epochs atomically when claiming and completing it.
    pub fn new(
        origin: &GatewayInstanceLease,
        target: &GatewayWorkerRecord,
        request_id: impl Into<String>,
        expires_at_ms: u64,
    ) -> GatewayRegistryResult<Self> {
        let request_id = request_id.into();
        if origin.tenant_id() != target.owner.tenant_id()
            || !valid_identifier(&request_id)
            || target.generation == 0
            || expires_at_ms == 0
        {
            return Err(GatewayRegistryError::InvalidContract);
        }
        Ok(Self {
            tenant_id: origin.tenant_id().clone(),
            request_id,
            origin_instance_id: origin.instance_id().clone(),
            origin_epoch: origin.epoch(),
            target_instance_id: target.owner.instance_id().clone(),
            target_cluster_id: target.owner.cluster_id().clone(),
            target_core_id: target.core_id.clone(),
            target_epoch: target.owner.epoch(),
            worker_generation: target.generation,
            expires_at_ms,
        })
    }

    /// Reports whether the request fence expired.
    pub const fn is_expired(&self, now_ms: u64) -> bool {
        now_ms >= self.expires_at_ms
    }
}

/// Controlled provider-independent HA registry failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum GatewayRegistryError {
    /// Shared registry reachability or command completion is uncertain.
    #[error("Gateway registry is unavailable")]
    Unavailable,
    /// Another live instance owns the selected resource.
    #[error("Gateway registry ownership conflicts with another instance")]
    Conflict,
    /// The supplied owner or generation fence is stale.
    #[error("Gateway registry fencing token is stale")]
    StaleOwner,
    /// The supplied lease or record expired.
    #[error("Gateway registry ownership expired")]
    Expired,
    /// A bounded provider capacity was exhausted.
    #[error("Gateway registry capacity is exhausted")]
    CapacityExceeded,
    /// Input or decoded record violates the V2 contract.
    #[error("Gateway registry contract is invalid")]
    InvalidContract,
    /// Shared storage contains another schema version.
    #[error("NO MORE SUPPORTED PLEASE UPDATE")]
    UnsupportedSchema,
}

/// Result returned by Gateway HA registry contracts.
pub type GatewayRegistryResult<T> = Result<T, GatewayRegistryError>;

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn valid_federation_url(value: &str) -> bool {
    if value.is_empty()
        || value.len() > MAX_GATEWAY_FEDERATION_URL_BYTES
        || value != value.trim()
        || value.contains(['@', '#', '?'])
    {
        return false;
    }
    let secure = value
        .strip_prefix("https://")
        .is_some_and(valid_url_authority);
    let loopback = value
        .strip_prefix("http://")
        .is_some_and(|authority| valid_url_authority(authority) && loopback_authority(authority));
    secure || loopback
}

fn valid_url_authority(value: &str) -> bool {
    if value.is_empty() || value.contains('/') {
        return false;
    }
    if let Some(rest) = value.strip_prefix('[') {
        let Some((host, port)) = rest.split_once(']') else {
            return false;
        };
        return !host.is_empty()
            && host
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() || byte == b':')
            && valid_optional_port(port);
    }
    let (host, port_valid) = value
        .split_once(':')
        .map_or((value, true), |(host, port)| (host, valid_port(port)));
    !host.is_empty()
        && host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.'))
        && port_valid
}

fn loopback_authority(value: &str) -> bool {
    value == "127.0.0.1"
        || value == "localhost"
        || value == "[::1]"
        || value.starts_with("127.0.0.1:")
        || value.starts_with("localhost:")
        || value.starts_with("[::1]:")
}

fn valid_optional_port(value: &str) -> bool {
    if value.is_empty() {
        return true;
    }
    valid_port(value.strip_prefix(':').unwrap_or(value))
}

fn valid_port(value: &str) -> bool {
    value.parse::<u16>().is_ok_and(|port| port != 0)
}

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
