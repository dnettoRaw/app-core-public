// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/29 20:47:35 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 00:04:12 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Storage contracts for provider health, repositories, migrations, and backups.

#![deny(missing_docs)]

pub mod manifest;
pub mod storage;

pub use manifest::{ManifestFileEntry, StorageManifest};
pub use storage::{
    data_claims, file_storage_capability_descriptor_v1, make_auth_request, now_ms,
    open_remote_request, open_remote_response, process_remote_request, seal_remote_request,
    seal_remote_response, transport_claims, validate_auth_resource, AuthRemoteRequest,
    AuthRemoteResponse, BackupDescriptor, DntFileObjectStore, DntFileSecretStore,
    DntFileSnapshotStore, FileStorageProvider, Migration, MigrationId, RemoteAuthStorageClient,
    Repository, RepositoryName, SealedObjectStore, SealedSecretStore, SealedSnapshotStore,
    SealedStoragePolicy, StorageCapabilityCatalogV1, StorageCapabilityDescriptorV1,
    StorageCapabilityError, StorageCapabilityProviderV1, StorageCapabilityRequirementsV1,
    StorageCapabilityV1, StorageError, StorageHealth, StorageProvider, StorageResult,
    StorageStatus, Transaction, AUTH_REMOTE_ENDPOINT, AUTH_REMOTE_SCHEMA,
    DEFAULT_AUTH_REMOTE_MAX_BYTES, DEFAULT_AUTH_REMOTE_TIMEOUT_MS, DEFAULT_AUTH_REMOTE_TTL_MS,
    MAX_STORAGE_CAPABILITY_PROVIDERS_V1, STORAGE_CAPABILITY_COUNT_V1,
    STORAGE_CAPABILITY_DESCRIPTOR_VERSION_V1, STORAGE_REQUIRED_CAPABILITIES_SETTING,
};
