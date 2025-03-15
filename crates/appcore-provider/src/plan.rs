// =============================================================================
//        #######
//     ###       ###     F: plan.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 10:59:21 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use crate::ProviderContext;
use appcore_contracts::{DeploymentManifestV1, ProviderConfig, ProviderId};
use std::collections::BTreeMap;

/// Immutable provider selections extracted from a deployment manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeploymentProviderPlan {
    context: ProviderContext,
    storage: ProviderConfig,
    control_plane: Option<ProviderConfig>,
    coordination_store: Option<ProviderConfig>,
    secret_provider: Option<ProviderConfig>,
    job_provider: Option<ProviderConfig>,
    peer_discovery: Option<ProviderConfig>,
    update: Option<ProviderConfig>,
    database: Option<ProviderConfig>,
    peer_transport: ProviderId,
    command_transport: ProviderId,
    adapters: BTreeMap<String, ProviderConfig>,
}

impl DeploymentProviderPlan {
    /// Extracts all provider ownership from a validated deployment manifest.
    pub fn from_manifest(manifest: &DeploymentManifestV1) -> Self {
        Self {
            context: ProviderContext::from_manifest(manifest),
            storage: manifest.storage().clone(),
            control_plane: manifest.control_plane().cloned(),
            coordination_store: manifest.coordination_store().cloned(),
            secret_provider: manifest.secret_provider().cloned(),
            job_provider: manifest.job_provider().cloned(),
            peer_discovery: manifest.peer_discovery().cloned(),
            update: manifest.update_provider().cloned(),
            database: manifest.database().cloned(),
            peer_transport: manifest.network().peer_transport().clone(),
            command_transport: manifest.network().command_transport().clone(),
            adapters: manifest.adapters().clone(),
        }
    }

    /// Returns common factory context.
    pub fn context(&self) -> &ProviderContext {
        &self.context
    }

    /// Returns the selected storage provider.
    pub fn storage(&self) -> &ProviderConfig {
        &self.storage
    }

    /// Returns the selected control-plane provider.
    pub fn control_plane(&self) -> Option<&ProviderConfig> {
        self.control_plane.as_ref()
    }

    /// Returns the selected coordination-store provider.
    pub fn coordination_store(&self) -> Option<&ProviderConfig> {
        self.coordination_store.as_ref()
    }

    /// Returns the selected installation secret provider.
    pub fn secret_provider(&self) -> Option<&ProviderConfig> {
        self.secret_provider.as_ref()
    }

    /// Returns the selected durable job provider.
    pub fn job_provider(&self) -> Option<&ProviderConfig> {
        self.job_provider.as_ref()
    }

    /// Returns the selected peer-discovery provider.
    pub fn peer_discovery(&self) -> Option<&ProviderConfig> {
        self.peer_discovery.as_ref()
    }

    /// Returns the selected update provider.
    pub fn update(&self) -> Option<&ProviderConfig> {
        self.update.as_ref()
    }

    /// Returns the selected application database provider.
    pub fn database(&self) -> Option<&ProviderConfig> {
        self.database.as_ref()
    }

    /// Returns the direct peer transport provider identity.
    pub fn peer_transport(&self) -> &ProviderId {
        &self.peer_transport
    }

    /// Returns the command transport provider identity.
    pub fn command_transport(&self) -> &ProviderId {
        &self.command_transport
    }

    /// Returns named installation adapters.
    pub fn adapters(&self) -> &BTreeMap<String, ProviderConfig> {
        &self.adapters
    }
}
