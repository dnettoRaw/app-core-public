// =============================================================================
//        #######
//     ###       ###     F: model.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::*;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeConfig {
    pub app_id: String,
    pub app_family: String,
    pub application_vendor: String,
    pub service_id: String,
    pub sync_group: String,
    pub node_id: String,
    pub tenant_id: String,
    pub cluster_id: String,
    pub core_id: String,
    pub instance_id: String,
    pub core_kind: String,
    pub protocol_version: u16,
    pub capabilities: Vec<String>,
    pub capability_requirements: HashMap<String, CapabilityRequirements>,
    pub storage_path: String,
    pub backup_path: String,
    pub api_enabled: bool,
    pub api_require_token: bool,
    pub api_public_status: bool,
    pub api_max_payload_bytes: usize,
    pub api_host: String,
    pub api_port: u16,
    pub sync_enabled: bool,
    pub sync_require_token: bool,
    pub sync_role: String,
    pub sync_bind_host: String,
    pub sync_bind_port: u16,
    pub sync_peers: Vec<String>,
    pub sync_dns_enabled: bool,
    pub sync_dns_seeds: Vec<String>,
    pub sync_dns_default_port: u16,
    pub sync_push_every_ticks: u64,
    pub security_provider: String,
    pub security_secret_path: String,
    pub security_secret_env: Option<String>,
    pub security_allow_expired_secret: bool,
    pub token_issuer: String,
    pub token_audience: String,
    pub token_ttl_ms: Option<u64>,
    pub idempotency_ttl_ms: u64,
    pub api_mdns_enabled: bool,
    pub api_mdns_service_name: String,
    pub control_plane_enabled: bool,
    pub control_plane_url: String,
    pub control_plane_heartbeat_interval_ms: u64,
    pub control_plane_request_timeout_ms: u64,
    pub control_plane_require_token: bool,
    pub control_plane_token_env: String,
    pub peer_rpc_enabled: bool,
    pub peer_rpc_bind_host: String,
    pub peer_rpc_bind_port: u16,
    pub operation_mode: RuntimeOperationalMode,
    pub runtime_mode: RuntimeMode,
    pub deployment_manifest_path: Option<String>,
    pub only_one: bool,
    pub kill_others: bool,
    pub supervisor_watchdog_enabled: bool,
    pub supervisor_watchdog_check_interval_ms: u64,
    pub supervisor_watchdog_stall_timeout_ms: u64,
}

#[derive(Debug)]
pub enum RuntimeConfigError {
    MissingField(&'static str),
    InvalidBool(&'static str),
    InvalidU16(&'static str),
    InvalidU64(&'static str),
    InvalidPath(&'static str),
    InvalidIdentifier(&'static str, String),
    InvalidCapabilityRequirement(String),
    InvalidOperationMode(String),
    InvalidValue(&'static str, String),
    Parse(String),
    UnknownField(String),
}

impl fmt::Display for RuntimeConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuntimeConfigError::MissingField(name) => write!(f, "missing field: {name}"),
            RuntimeConfigError::InvalidBool(name) => write!(f, "invalid bool field: {name}"),
            RuntimeConfigError::InvalidU16(name) => write!(f, "invalid u16 field: {name}"),
            RuntimeConfigError::InvalidU64(name) => write!(f, "invalid u64 field: {name}"),
            RuntimeConfigError::InvalidPath(name) => write!(f, "invalid path field: {name}"),
            RuntimeConfigError::InvalidIdentifier(name, value) => {
                write!(f, "invalid identifier field {name}: {value}")
            }
            RuntimeConfigError::InvalidCapabilityRequirement(value) => {
                write!(f, "invalid capability requirement: {value}")
            }
            RuntimeConfigError::InvalidOperationMode(value) => {
                write!(f, "invalid operation_mode: {value}")
            }
            RuntimeConfigError::InvalidValue(name, value) => {
                write!(f, "invalid value for {name}: {value}")
            }
            RuntimeConfigError::Parse(message) => write!(f, "invalid TOML: {message}"),
            RuntimeConfigError::UnknownField(name) => write!(f, "unknown field: {name}"),
        }
    }
}
