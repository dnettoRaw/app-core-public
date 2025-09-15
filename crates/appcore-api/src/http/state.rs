// =============================================================================
//        #######
//     ###       ###     F: state.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/02 13:08:16 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 11:51:10 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Shared HTTP runtime state and status metadata.

use crate::ApiRouter;
use appcore_core::{RuntimeController, RuntimeOperationalMode};
use parking_lot::Mutex;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use super::auth::HttpCommandAuth;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Runtime HTTP listener and request-size configuration.
pub struct HttpApiConfig {
    /// Interface or address to bind.
    pub host: String,
    /// TCP port to bind.
    pub port: u16,
    /// Whether the embedded listener should run.
    pub enabled: bool,
    /// Maximum accepted request body size in bytes.
    pub max_payload_bytes: usize,
}

impl Default for HttpApiConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
            enabled: false,
            max_payload_bytes: 65_536,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
/// Non-sensitive Runtime facts exposed by health and status routes.
pub struct RuntimeStaticInfo {
    /// Application identity hosted by this process.
    pub app_id: String,
    /// Runtime node identity.
    pub node_id: String,
    /// Tenant isolation boundary.
    pub tenant_id: String,
    /// Cluster isolation boundary.
    pub cluster_id: String,
    /// Logical Core identity.
    pub core_id: String,
    /// Initial operational-mode label.
    pub operation_mode: String,
    /// Storage provider health label.
    pub storage_status: String,
    /// Whether required security material initialized successfully.
    pub security_ok: bool,
    /// Whether HTTP ingress is enabled.
    pub api_enabled: bool,
    /// Whether synchronization is enabled.
    pub sync_enabled: bool,
    /// Local synchronization role.
    pub sync_role: String,
    /// Number of records currently visible in the sync log.
    pub sync_log_len: usize,
    /// Optional sync-log path for local diagnostics.
    pub sync_log_path: Option<String>,
    /// Optional sync-checkpoint path for local diagnostics.
    pub sync_checkpoint_path: Option<String>,
    /// Configured peer addresses without credentials.
    pub sync_peers: Vec<String>,
    /// Whether DNS peer discovery is enabled.
    pub sync_dns_enabled: bool,
    /// Configured DNS peer seeds.
    pub sync_dns_seeds: Vec<String>,
    /// Default port applied to DNS seeds.
    pub sync_dns_default_port: u16,
    /// Idempotency retention window in milliseconds.
    pub idempotency_ttl_ms: u64,
    /// Optional idempotency-store path for local diagnostics.
    pub idempotency_path: Option<String>,
}

#[derive(Clone)]
pub(crate) struct HttpState {
    pub(crate) static_info: RuntimeStaticInfo,
    pub(crate) controller: Option<Arc<Mutex<RuntimeController>>>,
    pub(crate) app_query_router: Option<Arc<Mutex<ApiRouter>>>,
    pub(crate) sync_log: Option<Arc<dyn SyncLogView>>,
    pub(crate) tick_counter: Option<Arc<AtomicU64>>,
    pub(crate) operation_mode: Option<Arc<Mutex<RuntimeOperationalMode>>>,
    pub(crate) command_policy: Option<Arc<dyn CommandCapabilityPolicy>>,
    pub(crate) supervisor: Option<appcore_supervisor::Supervisor>,
    pub(crate) auth: HttpCommandAuth,
    pub(crate) max_payload_bytes: usize,
    pub(crate) clock: Arc<dyn appcore_core::Clock>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Stable reason a capability policy rejected an invocation.
pub enum CommandCapabilityPolicyError {
    /// The invocation has no declared capability descriptor.
    CapabilityNotDeclared,
    /// The capability requires an idempotency key.
    MissingIdempotencyKey,
    /// The capability requires service leadership.
    RequiresLeader,
    /// The applicable service lease has expired.
    LeaseExpired,
    /// The request uses an obsolete fencing epoch.
    StaleEpoch,
    /// Current operational policy permits reads only.
    ReadOnly,
    /// Provider-specific policy rejected the invocation.
    Rejected(String),
}

/// Authorizes application invocations against capability and leadership policy.
pub trait CommandCapabilityPolicy: Send + Sync {
    /// Authorizes a named command at `now_ms`.
    fn authorize_command(
        &self,
        command_name: &str,
        idempotency_key: Option<&str>,
        now_ms: u64,
    ) -> Result<(), CommandCapabilityPolicyError>;

    /// Authorizes a named application query at `now_ms`.
    fn authorize_query(
        &self,
        _query_name: &str,
        _now_ms: u64,
    ) -> Result<(), CommandCapabilityPolicyError> {
        Ok(())
    }
}

/// Read-only synchronization-log metrics exposed to the HTTP host.
pub trait SyncLogView: Send + Sync {
    /// Returns the number of visible replication records.
    fn len(&self) -> usize;

    /// Reports whether no replication records are visible.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
