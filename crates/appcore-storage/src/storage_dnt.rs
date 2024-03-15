// =============================================================================
//        #######
//     ###       ###     F: storage_dnt.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/02 00:04:12 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:07:11 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! DNT-backed sealed storage adapters.

use super::{FileStorageProvider, StorageError, StorageResult};
use appcore_contracts::ApplicationId;
use appcore_dnt::{
    inspect_header, open_owned, rekey, seal, verify, ContentType, DntCodec, DntKeyProvider,
    DntOpenOptions, DntSealOptions, KeyId, DNT_FLAG_PAYLOAD_DEFLATE,
};
use appcore_types::TenantId;

/// Storage policy used when sealing objects into DNT envelopes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedStoragePolicy {
    /// Application that owns the object.
    pub application_id: ApplicationId,
    /// Optional tenant boundary.
    pub tenant_id: Option<TenantId>,
    /// Logical DNT content type.
    pub content_type: ContentType,
    /// Payload schema version.
    pub schema_version: u32,
    /// Key used for new writes.
    pub key_id: KeyId,
    /// Maximum encoded payload size.
    pub max_payload_bytes: u64,
    /// Authenticated public metadata.
    pub public_metadata: Vec<u8>,
}

impl SealedStoragePolicy {
    /// Creates DNT open options from this policy.
    pub fn open_options(&self) -> DntOpenOptions {
        DntOpenOptions {
            application_id: self.application_id.clone(),
            tenant_id: self.tenant_id.clone(),
            content_type: self.content_type.clone(),
            max_payload_bytes: Some(self.max_payload_bytes),
        }
    }

    fn seal_options(&self, created_at_ms: u64, encrypted_metadata: Vec<u8>) -> DntSealOptions {
        DntSealOptions {
            application_id: self.application_id.clone(),
            tenant_id: self.tenant_id.clone(),
            content_type: self.content_type.clone(),
            schema_version: self.schema_version,
            key_id: self.key_id.clone(),
            created_at_ms,
            public_metadata: self.public_metadata.clone(),
            encrypted_metadata,
            flags: 0,
            max_payload_bytes: Some(self.max_payload_bytes),
        }
    }
}

/// Generic sealed-object store contract.
pub trait SealedObjectStore {
    /// Seals and atomically writes an object.
    fn write_object(&self, path: &str, payload: &[u8]) -> StorageResult<()>;
    /// Reads, authenticates and opens an object.
    fn read_object(&self, path: &str) -> StorageResult<Vec<u8>>;
    /// Cryptographically verifies an object without returning plaintext.
    fn verify_object(&self, path: &str) -> StorageResult<()>;
    /// Re-encrypts an object under a new key.
    fn rekey_object(&self, path: &str, new_key_id: KeyId) -> StorageResult<()>;
}

/// Sealed snapshot store contract.
pub trait SealedSnapshotStore: SealedObjectStore {}

/// Sealed secret store contract.
pub trait SealedSecretStore: SealedObjectStore {}

/// DNT adapter over the local file storage provider.
#[derive(Debug)]
pub struct DntFileObjectStore<'a, K, C> {
    provider: &'a FileStorageProvider,
    key_provider: &'a K,
    codec: C,
    policy: SealedStoragePolicy,
}

impl<'a, K, C> DntFileObjectStore<'a, K, C>
where
    K: DntKeyProvider,
    C: DntCodec,
{
    /// Creates a sealed object store over an existing file provider.
    pub fn new(
        provider: &'a FileStorageProvider,
        key_provider: &'a K,
        codec: C,
        policy: SealedStoragePolicy,
    ) -> Self {
        Self {
            provider,
            key_provider,
            codec,
            policy,
        }
    }

    /// Seals, compacts and atomically writes an object.
    ///
    /// Compact writes use DNT's authenticated DEFLATE payload flag. Existing
    /// readers open compact and normal envelopes through the same `read_object`
    /// path as long as they provide a payload bound.
    pub fn write_object_compact(&self, path: &str, payload: &[u8]) -> StorageResult<()> {
        self.write_object_with_flags(path, payload, DNT_FLAG_PAYLOAD_DEFLATE)
    }

    fn write_object_with_flags(&self, path: &str, payload: &[u8], flags: u32) -> StorageResult<()> {
        let mut options = self.policy.seal_options(now_ms(), Vec::new());
        options.flags = flags;
        let envelope =
            seal(payload, self.key_provider, &self.codec, options).map_err(map_dnt_error)?;
        verify(
            &envelope,
            self.key_provider,
            &self.codec,
            &self.policy.open_options(),
        )
        .map_err(map_dnt_error)?;
        self.provider.write_bytes_atomic(path, &envelope)
    }
}

impl<K, C> SealedObjectStore for DntFileObjectStore<'_, K, C>
where
    K: DntKeyProvider,
    C: DntCodec,
{
    fn write_object(&self, path: &str, payload: &[u8]) -> StorageResult<()> {
        self.write_object_with_flags(path, payload, 0)
    }

    fn read_object(&self, path: &str) -> StorageResult<Vec<u8>> {
        let options = self.policy.open_options();
        let max_envelope_bytes = options.max_envelope_bytes().map_err(map_dnt_error)?;
        let bytes = self.provider.read_bytes_bounded(path, max_envelope_bytes)?;
        let opened =
            open_owned(bytes, self.key_provider, &self.codec, &options).map_err(map_dnt_error)?;
        Ok(opened.payload)
    }

    fn verify_object(&self, path: &str) -> StorageResult<()> {
        let options = self.policy.open_options();
        let max_envelope_bytes = options.max_envelope_bytes().map_err(map_dnt_error)?;
        let bytes = self.provider.read_bytes_bounded(path, max_envelope_bytes)?;
        verify(&bytes, self.key_provider, &self.codec, &options).map_err(map_dnt_error)?;
        Ok(())
    }

    fn rekey_object(&self, path: &str, new_key_id: KeyId) -> StorageResult<()> {
        let options = self.policy.open_options();
        let max_envelope_bytes = options.max_envelope_bytes().map_err(map_dnt_error)?;
        let bytes = self.provider.read_bytes_bounded(path, max_envelope_bytes)?;
        inspect_header(&bytes).map_err(map_dnt_error)?;
        let rotated = rekey(&bytes, self.key_provider, &self.codec, &options, new_key_id)
            .map_err(map_dnt_error)?;
        verify(&rotated, self.key_provider, &self.codec, &options).map_err(map_dnt_error)?;
        self.provider.write_bytes_atomic(path, &rotated)
    }
}

/// DNT file adapter for snapshots.
#[derive(Debug)]
pub struct DntFileSnapshotStore<'a, K, C>(DntFileObjectStore<'a, K, C>);

impl<'a, K, C> DntFileSnapshotStore<'a, K, C>
where
    K: DntKeyProvider,
    C: DntCodec,
{
    /// Creates a sealed snapshot store.
    pub fn new(inner: DntFileObjectStore<'a, K, C>) -> Self {
        Self(inner)
    }
}

impl<K, C> SealedObjectStore for DntFileSnapshotStore<'_, K, C>
where
    K: DntKeyProvider,
    C: DntCodec,
{
    fn write_object(&self, path: &str, payload: &[u8]) -> StorageResult<()> {
        self.0.write_object(path, payload)
    }

    fn read_object(&self, path: &str) -> StorageResult<Vec<u8>> {
        self.0.read_object(path)
    }

    fn verify_object(&self, path: &str) -> StorageResult<()> {
        self.0.verify_object(path)
    }

    fn rekey_object(&self, path: &str, new_key_id: KeyId) -> StorageResult<()> {
        self.0.rekey_object(path, new_key_id)
    }
}

impl<K, C> SealedSnapshotStore for DntFileSnapshotStore<'_, K, C>
where
    K: DntKeyProvider,
    C: DntCodec,
{
}

/// DNT file adapter for local secrets.
#[derive(Debug)]
pub struct DntFileSecretStore<'a, K, C>(DntFileObjectStore<'a, K, C>);

impl<'a, K, C> DntFileSecretStore<'a, K, C>
where
    K: DntKeyProvider,
    C: DntCodec,
{
    /// Creates a sealed secret store.
    pub fn new(inner: DntFileObjectStore<'a, K, C>) -> Self {
        Self(inner)
    }
}

impl<K, C> SealedObjectStore for DntFileSecretStore<'_, K, C>
where
    K: DntKeyProvider,
    C: DntCodec,
{
    fn write_object(&self, path: &str, payload: &[u8]) -> StorageResult<()> {
        self.0.write_object(path, payload)
    }

    fn read_object(&self, path: &str) -> StorageResult<Vec<u8>> {
        self.0.read_object(path)
    }

    fn verify_object(&self, path: &str) -> StorageResult<()> {
        self.0.verify_object(path)
    }

    fn rekey_object(&self, path: &str, new_key_id: KeyId) -> StorageResult<()> {
        self.0.rekey_object(path, new_key_id)
    }
}

impl<K, C> SealedSecretStore for DntFileSecretStore<'_, K, C>
where
    K: DntKeyProvider,
    C: DntCodec,
{
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn map_dnt_error(error: appcore_dnt::DntError) -> StorageError {
    match error {
        appcore_dnt::DntError::Io => StorageError::TransactionFailed("dnt".to_string()),
        appcore_dnt::DntError::KeyUnavailable
        | appcore_dnt::DntError::AuthenticationFailed
        | appcore_dnt::DntError::ContextMismatch => StorageError::SecurityFailed("dnt".to_string()),
        _ => StorageError::InvalidPath("dnt".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use appcore_dnt::{
        BytesCodec, SecretKey, StaticDntKeyProvider, DNT_CONTENT_OCTET_STREAM, DNT_CONTENT_SECRET,
    };

    fn provider() -> StaticDntKeyProvider {
        StaticDntKeyProvider::new()
            .with_key(KeyId::new("key-a").unwrap(), SecretKey::new([3; 32]))
            .with_key(KeyId::new("key-b").unwrap(), SecretKey::new([4; 32]))
    }

    fn storage() -> FileStorageProvider {
        let root = std::env::temp_dir().join(format!("appcore-storage-dnt-{}", unique()));
        let data = root.join("data");
        let backups = root.join("backups");
        let provider = FileStorageProvider::new(data, backups);
        provider.create_dirs().unwrap();
        provider
    }

    fn policy(content_type: &str) -> SealedStoragePolicy {
        SealedStoragePolicy {
            application_id: ApplicationId::new("app-a").unwrap(),
            tenant_id: Some(TenantId::new("tenant-a").unwrap()),
            content_type: ContentType::new(content_type).unwrap(),
            schema_version: 1,
            key_id: KeyId::new("key-a").unwrap(),
            max_payload_bytes: 1024 * 1024,
            public_metadata: b"store=sealed".to_vec(),
        }
    }

    #[test]
    fn sealed_object_store_keeps_plaintext_off_disk() {
        let storage = storage();
        let keys = provider();
        let store = DntFileObjectStore::new(
            &storage,
            &keys,
            BytesCodec,
            policy(DNT_CONTENT_OCTET_STREAM),
        );

        store
            .write_object("objects/a.dntb", b"secret payload")
            .unwrap();
        let raw = storage.read_bytes("objects/a.dntb").unwrap();

        assert!(!raw
            .windows(b"secret payload".len())
            .any(|w| w == b"secret payload"));
        assert_eq!(
            store.read_object("objects/a.dntb").unwrap(),
            b"secret payload"
        );
        assert_eq!(store.verify_object("objects/a.dntb"), Ok(()));
    }

    #[test]
    fn sealed_object_store_can_write_compact_dnt() {
        let storage = storage();
        let keys = provider();
        let store = DntFileObjectStore::new(
            &storage,
            &keys,
            BytesCodec,
            policy(DNT_CONTENT_OCTET_STREAM),
        );
        let payload = b"snapshot-line=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n".repeat(2048);

        store.write_object("objects/normal.dntb", &payload).unwrap();
        store
            .write_object_compact("objects/compact.dntb", &payload)
            .unwrap();
        let normal = storage.read_bytes("objects/normal.dntb").unwrap();
        let compact = storage.read_bytes("objects/compact.dntb").unwrap();

        assert!(compact.len() < normal.len() / 10);
        assert_eq!(store.read_object("objects/compact.dntb").unwrap(), payload);
        assert_eq!(store.verify_object("objects/compact.dntb"), Ok(()));
    }

    #[test]
    fn sealed_object_rekey_preserves_payload() {
        let storage = storage();
        let keys = provider();
        let store = DntFileObjectStore::new(
            &storage,
            &keys,
            BytesCodec,
            policy(DNT_CONTENT_OCTET_STREAM),
        );

        store.write_object("objects/a.dntb", b"payload").unwrap();
        store
            .rekey_object("objects/a.dntb", KeyId::new("key-b").unwrap())
            .unwrap();

        let raw = storage.read_bytes("objects/a.dntb").unwrap();
        assert_eq!(inspect_header(&raw).unwrap().key_id.as_str(), "key-b");
        assert_eq!(store.read_object("objects/a.dntb").unwrap(), b"payload");
    }

    #[test]
    fn sealed_object_rejects_oversized_envelope_before_opening() {
        let storage = storage();
        let keys = provider();
        let policy = policy(DNT_CONTENT_OCTET_STREAM);
        let max_envelope = policy.open_options().max_envelope_bytes().unwrap();
        let store = DntFileObjectStore::new(&storage, &keys, BytesCodec, policy);
        storage
            .write_bytes_atomic(
                "objects/oversized.dntb",
                &vec![0; max_envelope as usize + 1],
            )
            .unwrap();

        assert!(matches!(
            store.read_object("objects/oversized.dntb"),
            Err(StorageError::TransactionFailed(_))
        ));
        assert!(matches!(
            store.verify_object("objects/oversized.dntb"),
            Err(StorageError::TransactionFailed(_))
        ));
    }

    #[test]
    fn sealed_secret_store_rejects_wrong_content_type() {
        let storage = storage();
        let keys = provider();
        let object_store = DntFileObjectStore::new(
            &storage,
            &keys,
            BytesCodec,
            policy(DNT_CONTENT_OCTET_STREAM),
        );
        object_store
            .write_object("secrets/a.dnt", b"payload")
            .unwrap();

        let secret_store =
            DntFileObjectStore::new(&storage, &keys, BytesCodec, policy(DNT_CONTENT_SECRET));
        assert!(matches!(
            secret_store.read_object("secrets/a.dnt"),
            Err(StorageError::SecurityFailed(_))
        ));
    }

    fn unique() -> u64 {
        // appcore-norm: allow(global-state) reason: atomic sequence prevents process-local temporary path collisions
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let count = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        ((std::process::id() as u64) << 32) | count
    }
}
