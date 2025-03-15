// =============================================================================
//        #######
//     ###       ###     F: context.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/22 15:41:18 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use appcore_contracts::{ApplicationId, DeploymentManifestV1, InstallationId, RuntimeMode};

/// Non-secret context supplied to every provider factory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderContext {
    application_id: ApplicationId,
    installation_id: InstallationId,
    runtime_mode: RuntimeMode,
}

impl ProviderContext {
    /// Creates context from a validated deployment manifest.
    pub fn from_manifest(manifest: &DeploymentManifestV1) -> Self {
        Self {
            application_id: manifest.application_id().clone(),
            installation_id: manifest.installation_id().clone(),
            runtime_mode: manifest.mode(),
        }
    }

    /// Returns the installed application identity.
    pub fn application_id(&self) -> &ApplicationId {
        &self.application_id
    }

    /// Returns the installation identity.
    pub fn installation_id(&self) -> &InstallationId {
        &self.installation_id
    }

    /// Returns the explicit runtime mode.
    pub fn runtime_mode(&self) -> RuntimeMode {
        self.runtime_mode
    }
}
