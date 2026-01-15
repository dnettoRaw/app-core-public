// =============================================================================
//        #######
//     ###       ###     F: config.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/26 08:53:09 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:48:56 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Gateway configuration definitions.

use appcore_contracts::ProviderConfig;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::time::Duration;

/// Deployment adapter key that enables the Runtime-owned Gateway service.
pub const GATEWAY_ADAPTER_NAME: &str = "gateway";
/// Provider identity implemented by this crate.
pub const GATEWAY_PROVIDER_ID: &str = "appcore-gateway";

/// Maximum decoded WebSocket message or frame size.
pub const MAX_GATEWAY_MESSAGE_BYTES: usize = 4 * 1024 * 1024;
/// Maximum JSON body size for the mesh relay's byte-array encoding.
pub const MAX_GATEWAY_HTTP_BODY_BYTES: usize = MAX_GATEWAY_MESSAGE_BYTES * 4 + 65_536;
/// Maximum capabilities accepted from one worker connection.
pub const MAX_GATEWAY_CAPABILITIES: usize = 64;
/// Maximum active workers retained in one tenant partition.
pub const MAX_GATEWAY_WORKERS_PER_TENANT: usize = 1_024;
/// Maximum active clients retained in one tenant partition.
pub const MAX_GATEWAY_CLIENTS_PER_TENANT: usize = 4_096;
/// Maximum simultaneous connections accepted by one Gateway process.
pub const MAX_GATEWAY_CONNECTIONS: usize = 8_192;
/// Maximum pending worker requests retained in one tenant partition.
pub const MAX_GATEWAY_PENDING_PER_TENANT: usize = 2_048;
/// Maximum tenant partitions retained by one Gateway process.
pub const MAX_GATEWAY_TENANTS: usize = 1_024;
/// Maximum timeout accepted from an untrusted relay request.
pub const MAX_GATEWAY_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Configuration options for the AppCore Gateway service.
///
/// The `domain_suffix` field **must** be set explicitly by the deployment.
/// AppCore is a generic Runtime and does not assume any specific domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayConfig {
    /// Server IP and port to bind to.
    pub bind_address: SocketAddr,

    /// Domain suffix used to resolve tenant IDs from incoming `Host` headers.
    ///
    /// The deployer sets this to the domain that owns the gateway
    /// (e.g., `gateway.example.com`). An incoming request to
    /// `tenant-a.gateway.example.com` resolves to `TenantId("tenant-a")`.
    ///
    /// This value is deployment-specific and has no default.
    pub domain_suffix: String,

    /// Whether the gateway enforces token authentication for connections.
    require_auth: bool,

    /// Interval at which the gateway expects worker heartbeats.
    pub heartbeat_interval: Duration,

    /// Maximum time window allowed without any heartbeat before pruning a connection.
    pub heartbeat_timeout: Duration,
}

impl GatewayConfig {
    /// Creates a configuration with all required fields.
    pub fn new(bind_address: SocketAddr, domain_suffix: impl Into<String>) -> Self {
        Self {
            bind_address,
            domain_suffix: domain_suffix.into(),
            require_auth: true,
            heartbeat_interval: Duration::from_secs(30),
            heartbeat_timeout: Duration::from_secs(90),
        }
    }

    /// Reports whether connection and mesh-relay authentication is required.
    pub fn requires_authentication(&self) -> bool {
        self.require_auth
    }

    /// Parses the bounded, non-secret Gateway settings selected by a
    /// deployment adapter.
    ///
    /// Supported settings are `bind_address`, `domain_suffix`,
    /// `heartbeat_interval_ms`, and `heartbeat_timeout_ms`. Authentication is
    /// intentionally not configurable through a deployment setting and
    /// remains enabled.
    pub fn from_provider_config(provider: &ProviderConfig) -> crate::GatewayResult<Self> {
        validate_provider_shape(provider)?;
        let settings = provider.settings();
        let bind_address = required_setting(settings, "bind_address")?
            .parse::<SocketAddr>()
            .map_err(|error| {
                crate::GatewayError::Config(format!("invalid bind_address: {error}"))
            })?;
        let domain_suffix = required_setting(settings, "domain_suffix")?.to_string();
        let mut config = Self::new(bind_address, domain_suffix);
        config.heartbeat_interval =
            duration_setting(settings, "heartbeat_interval_ms", config.heartbeat_interval)?;
        config.heartbeat_timeout =
            duration_setting(settings, "heartbeat_timeout_ms", config.heartbeat_timeout)?;
        config.validate()?;
        Ok(config)
    }

    /// Explicitly disables authentication for a loopback-only local test.
    pub fn insecure_local_for_testing(mut self) -> Result<Self, crate::error::GatewayError> {
        if !self.bind_address.ip().is_loopback() {
            return Err(crate::error::GatewayError::Config(
                "insecure gateway mode requires a loopback bind address".to_string(),
            ));
        }
        self.require_auth = false;
        Ok(self)
    }

    /// Validates the configuration bounds.
    pub fn validate(&self) -> Result<(), crate::error::GatewayError> {
        if !valid_domain_suffix(&self.domain_suffix) {
            return Err(crate::error::GatewayError::Config(
                "domain_suffix must be an explicit valid DNS suffix".to_string(),
            ));
        }
        if self.heartbeat_interval.is_zero() {
            return Err(crate::error::GatewayError::Config(
                "heartbeat_interval must be greater than zero".to_string(),
            ));
        }
        if self.heartbeat_timeout <= self.heartbeat_interval {
            return Err(crate::error::GatewayError::Config(
                "heartbeat_timeout must be strictly greater than heartbeat_interval".to_string(),
            ));
        }
        if !self.require_auth && !self.bind_address.ip().is_loopback() {
            return Err(crate::error::GatewayError::Config(
                "gateway authentication cannot be disabled on a non-loopback bind address"
                    .to_string(),
            ));
        }
        Ok(())
    }
}

fn validate_provider_shape(provider: &ProviderConfig) -> crate::GatewayResult<()> {
    if provider.provider_id().as_str() != GATEWAY_PROVIDER_ID {
        return Err(crate::GatewayError::Config(format!(
            "gateway adapter requires provider_id={GATEWAY_PROVIDER_ID}"
        )));
    }
    if provider.endpoint().is_some() {
        return Err(crate::GatewayError::Config(
            "gateway adapter does not accept a provider endpoint".to_string(),
        ));
    }
    if !provider.secret_refs().is_empty() {
        return Err(crate::GatewayError::Config(
            "gateway adapter reuses Runtime security and accepts no secret refs".to_string(),
        ));
    }
    const SETTINGS: [&str; 4] = [
        "bind_address",
        "domain_suffix",
        "heartbeat_interval_ms",
        "heartbeat_timeout_ms",
    ];
    if let Some(name) = provider
        .settings()
        .keys()
        .find(|name| !SETTINGS.contains(&name.as_str()))
    {
        return Err(crate::GatewayError::Config(format!(
            "unsupported gateway setting: {name}"
        )));
    }
    Ok(())
}

fn required_setting<'a>(
    settings: &'a BTreeMap<String, String>,
    name: &'static str,
) -> crate::GatewayResult<&'a str> {
    settings
        .get(name)
        .map(String::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| crate::GatewayError::Config(format!("gateway requires {name}")))
}

fn duration_setting(
    settings: &BTreeMap<String, String>,
    name: &'static str,
    default: Duration,
) -> crate::GatewayResult<Duration> {
    let Some(value) = settings.get(name) else {
        return Ok(default);
    };
    value
        .parse::<u64>()
        .map(Duration::from_millis)
        .map_err(|_| crate::GatewayError::Config(format!("{name} must be a u64")))
}

fn valid_domain_suffix(value: &str) -> bool {
    if value.is_empty() || value.len() > 253 || value != value.trim() {
        return false;
    }
    value.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}
