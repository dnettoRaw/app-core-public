// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/06 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/06 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Verified, bounded transfer. Trust in descriptors and source selection belongs to deployment.
//! Download completion never installs an artifact or changes replication state.
mod control;
mod descriptor;
mod engine;
mod error;
mod filesystem;
#[cfg(windows)]
mod filesystem_windows;
#[cfg(feature = "peer-rpc")]
pub mod peer;
mod store;
pub use control::TransferControl;
pub use descriptor::{ObjectDescriptor, TransferLimits};
pub use engine::{transfer, BlockSource, SourceBinding, TransferOptions, TransferReport};
pub use error::{TransferError, TransferResult};
pub use store::{TransferSession, TransferStore, VerifiedObject};
