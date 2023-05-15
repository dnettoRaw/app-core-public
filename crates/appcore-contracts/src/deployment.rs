// =============================================================================
//        #######
//     ###       ###     F: deployment.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/21 23:21:21 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 13:18:47 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Installation-owned deployment manifest contract.

use crate::identifiers::{is_sensitive_key, validate_text};
use crate::{
    ApplicationId, ContractError, ContractResult, InstallationId, ProviderId, RuntimeMode,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod config;
mod manifest;

pub use config::{
    DeploymentSupervisorConfig, DeploymentWatchdogConfig, EnvironmentBinding, NetworkConfig,
    ProviderConfig, SecretRef, TlsConfig, VolumeMount, DEPLOYMENT_MANIFEST_VERSION,
};
pub use manifest::{DeploymentManifestBuilder, DeploymentManifestV1};

#[cfg(test)]
mod tests;
