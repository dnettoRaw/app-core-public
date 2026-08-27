// =============================================================================
//        #######
//     ###       ###     F: ownership.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.6-rc
// =============================================================================

//! Bounded local ownership snapshot replayed after a fresh instance epoch.

use super::{GatewayHaTenantBinding, GatewayRegistryError, GatewayRegistryResult};
use crate::config::MAX_GATEWAY_CONNECTIONS;
use crate::GatewayWorkerRegistration;
use appcore_types::{ClusterId, TenantId};
use std::collections::HashSet;

/// One local worker that must be re-registered under a fresh tenant epoch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayHaWorkerSnapshot {
    /// Tenant isolation boundary.
    pub tenant_id: TenantId,
    /// Cluster boundary authenticated for the local socket.
    pub cluster_id: ClusterId,
    /// Exact local worker identity, generation and capabilities.
    pub registration: GatewayWorkerRegistration,
}

/// One authenticated local session that must be re-registered after recovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayHaSessionSnapshot {
    /// Tenant isolation boundary.
    pub tenant_id: TenantId,
    /// Bounded opaque session identity.
    pub session_id: String,
    /// Absolute authentication expiry.
    pub expires_at_ms: u64,
}

/// Complete bounded local ownership captured while HA admission is closed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GatewayHaOwnershipSnapshot {
    /// Current local worker sockets.
    pub workers: Vec<GatewayHaWorkerSnapshot>,
    /// Current authenticated client sessions.
    pub sessions: Vec<GatewayHaSessionSnapshot>,
}

impl GatewayHaOwnershipSnapshot {
    /// Validates bounds, configured tenants and unique local identities.
    pub fn validate(
        &self,
        tenants: &[GatewayHaTenantBinding],
        now_ms: u64,
    ) -> GatewayRegistryResult<()> {
        if self.workers.len().saturating_add(self.sessions.len()) > MAX_GATEWAY_CONNECTIONS {
            return Err(GatewayRegistryError::CapacityExceeded);
        }
        let configured = tenants
            .iter()
            .map(|binding| binding.tenant_id.as_str())
            .collect::<HashSet<_>>();
        let mut workers = HashSet::with_capacity(self.workers.len());
        for worker in &self.workers {
            worker.registration.validate()?;
            let configured_cluster = tenants
                .iter()
                .find(|binding| binding.tenant_id == worker.tenant_id)
                .map(|binding| &binding.cluster_id);
            if configured_cluster != Some(&worker.cluster_id)
                || !workers.insert(format!(
                    "{}\0{}\0{}",
                    worker.tenant_id.as_str(),
                    worker.registration.installation_id.as_str(),
                    worker.registration.core_id.as_str()
                ))
            {
                return Err(GatewayRegistryError::InvalidContract);
            }
        }
        let mut sessions = HashSet::with_capacity(self.sessions.len());
        for session in &self.sessions {
            if !configured.contains(session.tenant_id.as_str())
                || session.expires_at_ms <= now_ms
                || !valid_identifier(&session.session_id)
                || !sessions.insert((session.tenant_id.as_str(), session.session_id.as_str()))
            {
                return Err(GatewayRegistryError::InvalidContract);
            }
        }
        Ok(())
    }
}

/// Supplies one point-in-time local ownership snapshot during recovery.
pub trait GatewayHaOwnershipSource: Send + Sync {
    /// Captures all live workers and unexpired authenticated sessions.
    fn snapshot(&self, now_ms: u64) -> GatewayRegistryResult<GatewayHaOwnershipSnapshot>;
}

impl GatewayHaOwnershipSource for GatewayHaOwnershipSnapshot {
    fn snapshot(&self, _now_ms: u64) -> GatewayRegistryResult<GatewayHaOwnershipSnapshot> {
        Ok(self.clone())
    }
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

#[cfg(test)]
#[path = "ownership_tests.rs"]
mod tests;
