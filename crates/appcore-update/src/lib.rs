// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 13:21:42 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 13:24:05 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Application artifact update, activation and rollback lifecycle.
//!
//! The Runtime treats artifacts as opaque bytes. It verifies generic identity,
//! compatibility and integrity but never knows application code or business
//! data.

#![deny(missing_docs)]

mod artifact;
mod authenticity;
mod coordinator;
mod error;
mod filesystem;
mod integrity;
mod provider;
mod store;

pub use artifact::ArtifactDescriptor;
#[cfg(feature = "allow-unsigned-local-artifacts")]
pub use authenticity::UnsignedLocalArtifactVerifier;
pub use authenticity::{
    artifact_signing_payload, ArtifactAuthenticityVerifier, ArtifactTrustPolicy,
    Ed25519ArtifactVerifier, PolicyArtifactVerifier, SigningKeyStatus,
};
pub use coordinator::{
    ActivationHealthCheck, NoUpdateFaults, UpdateCoordinator, UpdateFaultInjector,
    UpdateFaultPoint, UpdateOutcome, UpdatePreparation, UpdateStaging,
};
pub use error::{UpdateError, UpdateResult};
pub use provider::{
    FileUpdateProvider, FileUpdateProviderFactory, SharedUpdateProvider, UpdateProvider,
    UpdateRequest, FILE_UPDATE_PROVIDER_ID,
};
pub use store::{
    ActivationReceipt, ArtifactStore, FileArtifactStore, StagedArtifact,
    UPDATE_METADATA_FORMAT_VERSION,
};

pub(crate) use integrity::sha256_hex;

#[cfg(test)]
mod tests;
