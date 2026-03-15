// =============================================================================
//        #######
//     ###       ###     F: runtime_config.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/31 13:38:42 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Internal host configuration derived exclusively from versioned manifests.

use appcore_contracts::RuntimeMode;
use appcore_core::{
    CapabilityRequirements, ClusterId, CoreId, CoreIdentity, CoreKind, InstanceId, ProtocolVersion,
    RuntimeIdentity, RuntimeOperationalMode, TenantId,
};
use std::ffi::OsString;
use std::fmt;
use std::path::{Component, Path, PathBuf};

mod model;
mod path;
mod validation;

pub(crate) use model::RuntimeConfig;
pub use model::RuntimeConfigError;
pub(crate) use path::{resolve_runtime_path, sanitize_distributed_default};
