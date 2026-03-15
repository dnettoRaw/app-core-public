// =============================================================================
//        #######
//     ###       ###     F: validation.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 13:18:47 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::*;

impl RuntimeConfig {
    pub fn core_identity(&self) -> Result<CoreIdentity, RuntimeConfigError> {
        Ok(CoreIdentity {
            tenant_id: TenantId::new(self.tenant_id.clone()).map_err(|_| {
                RuntimeConfigError::InvalidIdentifier("tenant_id", self.tenant_id.clone())
            })?,
            cluster_id: ClusterId::new(self.cluster_id.clone()).map_err(|_| {
                RuntimeConfigError::InvalidIdentifier("cluster_id", self.cluster_id.clone())
            })?,
            core_id: CoreId::new(self.core_id.clone()).map_err(|_| {
                RuntimeConfigError::InvalidIdentifier("core_id", self.core_id.clone())
            })?,
            instance_id: InstanceId::new(self.instance_id.clone()).map_err(|_| {
                RuntimeConfigError::InvalidIdentifier("instance_id", self.instance_id.clone())
            })?,
            kind: CoreKind::new(self.core_kind.clone()).map_err(|_| {
                RuntimeConfigError::InvalidIdentifier("core_kind", self.core_kind.clone())
            })?,
            protocol_version: ProtocolVersion::new(self.protocol_version),
            runtime: RuntimeIdentity {
                app_id: appcore_core::AppId::new(self.app_id.clone()).map_err(|_| {
                    RuntimeConfigError::InvalidIdentifier("app_id", self.app_id.clone())
                })?,
                app_family: appcore_core::AppFamily::new(self.app_family.clone()).map_err(
                    |_| {
                        RuntimeConfigError::InvalidIdentifier("app_family", self.app_family.clone())
                    },
                )?,
                sync_group: appcore_core::SyncGroup::new(self.sync_group.clone()).map_err(
                    |_| {
                        RuntimeConfigError::InvalidIdentifier("sync_group", self.sync_group.clone())
                    },
                )?,
                runtime_contract: appcore_core::RuntimeContractVersion::new(1),
                node_id: appcore_core::NodeId::new(self.node_id.clone()).map_err(|_| {
                    RuntimeConfigError::InvalidIdentifier("node_id", self.node_id.clone())
                })?,
            },
        })
    }

    /// Validates cross-field invariants after parsing and environment overrides.
    pub fn validate(&self) -> Result<(), RuntimeConfigError> {
        let _ = self.core_identity()?;
        if self.api_enabled && self.api_host.trim().is_empty() {
            return Err(RuntimeConfigError::InvalidValue(
                "api_host",
                self.api_host.clone(),
            ));
        }
        appcore_contracts::ServiceId::new(self.service_id.clone()).map_err(|_| {
            RuntimeConfigError::InvalidIdentifier("service_id", self.service_id.clone())
        })?;
        if self.application_vendor.trim().is_empty() {
            return Err(RuntimeConfigError::InvalidValue(
                "application_vendor",
                self.application_vendor.clone(),
            ));
        }
        match self.runtime_mode {
            RuntimeMode::Standalone if self.control_plane_enabled => {
                return Err(RuntimeConfigError::InvalidValue(
                    "runtime_mode",
                    "standalone forbids control_plane_enabled=true".to_string(),
                ));
            }
            RuntimeMode::Cluster if !self.control_plane_enabled => {
                return Err(RuntimeConfigError::InvalidValue(
                    "runtime_mode",
                    "cluster requires control_plane_enabled=true".to_string(),
                ));
            }
            RuntimeMode::Standalone | RuntimeMode::Cluster => {}
        }
        if self.api_max_payload_bytes == 0 {
            return Err(RuntimeConfigError::InvalidValue(
                "api_max_payload_bytes",
                self.api_max_payload_bytes.to_string(),
            ));
        }
        if self.sync_enabled {
            if !matches!(self.sync_role.as_str(), "leader" | "follower") {
                return Err(RuntimeConfigError::InvalidValue(
                    "sync_role",
                    self.sync_role.clone(),
                ));
            }
            if self.sync_bind_host.trim().is_empty() {
                return Err(RuntimeConfigError::InvalidValue(
                    "sync_bind_host",
                    self.sync_bind_host.clone(),
                ));
            }
            if self.sync_push_every_ticks == 0 {
                return Err(RuntimeConfigError::InvalidValue(
                    "sync_push_every_ticks",
                    self.sync_push_every_ticks.to_string(),
                ));
            }
        }
        if self.security_provider != "hashtoken" {
            return Err(RuntimeConfigError::InvalidValue(
                "security_provider",
                self.security_provider.clone(),
            ));
        }
        if self.token_issuer.trim().is_empty() {
            return Err(RuntimeConfigError::InvalidValue(
                "token_issuer",
                self.token_issuer.clone(),
            ));
        }
        if self.token_audience.trim().is_empty() {
            return Err(RuntimeConfigError::InvalidValue(
                "token_audience",
                self.token_audience.clone(),
            ));
        }
        if self.supervisor_watchdog_enabled
            && self.supervisor_watchdog_stall_timeout_ms
                <= self.supervisor_watchdog_check_interval_ms
        {
            return Err(RuntimeConfigError::InvalidValue(
                "supervisor_watchdog_stall_timeout_ms",
                "must exceed supervisor_watchdog_check_interval_ms".to_string(),
            ));
        }
        Ok(())
    }
}
