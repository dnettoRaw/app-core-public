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

mod activation;
mod artifact;
mod authenticity;
mod cache;
mod catalog;
mod catalog_store;
mod coordinator;
mod error;
mod filesystem;
mod integrity;
mod mobile;
mod peer;
mod provider;
mod quarantine;
mod recovery;
mod store;
mod store_io;
mod stream;

pub use activation::{ActivationAdapter, ActivationEvidence, ActivationRequest};
pub use artifact::{ArtifactDescriptor, ArtifactTarget};
#[cfg(feature = "allow-unsigned-local-artifacts")]
pub use authenticity::UnsignedLocalArtifactVerifier;
pub use authenticity::{
    artifact_signing_payload, ArtifactAuthenticityVerifier, ArtifactTrustPolicy,
    Ed25519ArtifactVerifier, PolicyArtifactVerifier, SigningKeyStatus,
};
pub use cache::{CacheOptions, CachedArtifact, UpdateCache};
pub use catalog::{
    LatestCompatibleUpdateIdentity, QuarantineExclusion, QuarantineSelection, ReleaseCatalog,
    UpdateIdentity, RELEASE_CATALOG_MAX_BYTES, RELEASE_CATALOG_MAX_ENTRIES,
};
pub use catalog_store::{CatalogEntry, ReleaseCatalogStore};
pub use coordinator::{
    ActivationHealthCheck, NoUpdateFaults, UpdateCoordinator, UpdateFaultInjector,
    UpdateFaultPoint, UpdateOutcome, UpdatePreparation, UpdateStaging,
};
pub use error::{UpdateError, UpdateResult};
pub use mobile::{
    MobileUpdateAction, MobileUpdateDecision, MobileUpdatePolicy, MobileUpdateRequest,
};
pub use peer::{
    ArtifactChunkRequestV2, ArtifactChunkResponseV2, ArtifactOfferRequestV2,
    ArtifactOfferResponseV2, ArtifactPeerStatusV2, LatestCompatibleOfferRequestV2,
    LatestCompatibleOfferResponseV2, UPDATE_CHUNK_CAPABILITY, UPDATE_OFFER_CAPABILITY,
    UPDATE_PEER_METADATA_MAX_BYTES,
};
pub use provider::{
    FileUpdateProvider, FileUpdateProviderFactory, SharedUpdateProvider, UpdateProvider,
    UpdateRequest, FILE_UPDATE_PROVIDER_ID,
};
pub use quarantine::{
    QuarantineKey, QuarantineReason, QuarantineRecord, QuarantineState, QuarantineStore,
    QUARANTINE_FORMAT_VERSION, QUARANTINE_MAX_ENTRIES,
};
pub use recovery::{
    ActivationPhaseV2, ActivationReceiptV2, FileRecoveryStore, RecoveryAction, RecoveryDecision,
    RecoveryResult, ACTIVATION_V2_FORMAT_VERSION,
};
pub use store::{
    ActivationReceipt, ArtifactStore, FileArtifactStore, StagedArtifact,
    UPDATE_METADATA_FORMAT_VERSION,
};
pub use stream::{
    receive_artifact, ArtifactSource, ArtifactTransferPolicy, ArtifactWriter, FileArtifactSource,
    DEFAULT_ARTIFACT_CHUNK_BYTES,
};

pub(crate) use integrity::sha256_hex;

#[cfg(test)]
mod tests;
