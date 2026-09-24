// =============================================================================
//        #######
//     ###       ###     F: tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 13:24:05 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;
use appcore_contracts::{ApplicationId, BuildId};
use ed25519_dalek::{Signer, SigningKey};
use std::fs;
use std::io::{self, Cursor, Read, Write};
use std::sync::Mutex;

use crate::store::StoreFaultPoint;

fn descriptor(version: &str, build: &str, bytes: &[u8]) -> ArtifactDescriptor {
    ArtifactDescriptor::new(
        ApplicationId::new("app-a").unwrap(),
        version,
        BuildId::new(build).unwrap(),
        "stable",
        ">=0.6.0, <1.0.0",
        "1",
        format!("memory:{build}"),
        sha256_hex(bytes),
        bytes.len() as u64,
    )
    .unwrap()
}

struct MemoryProvider {
    descriptor: ArtifactDescriptor,
    bytes: Vec<u8>,
}

impl UpdateProvider for MemoryProvider {
    fn latest(&self, _request: &UpdateRequest) -> UpdateResult<Option<ArtifactDescriptor>> {
        Ok(Some(self.descriptor.clone()))
    }

    fn fetch(&self, _artifact: &ArtifactDescriptor, max_bytes: usize) -> UpdateResult<Vec<u8>> {
        if self.bytes.len() > max_bytes {
            return Err(UpdateError::ArtifactTooLarge { max_bytes });
        }
        Ok(self.bytes.clone())
    }
}

struct Healthy(bool);

impl ActivationHealthCheck for Healthy {
    fn check(&self, _artifact: &ArtifactDescriptor) -> UpdateResult<()> {
        if self.0 {
            Ok(())
        } else {
            Err(UpdateError::Health("probe failed".to_string()))
        }
    }
}

struct ChunkSource {
    bytes: Vec<u8>,
    oversized: bool,
}

impl ArtifactSource for ChunkSource {
    fn read_chunk(
        &self,
        _descriptor: &ArtifactDescriptor,
        offset: u64,
        max_len: usize,
    ) -> UpdateResult<Vec<u8>> {
        let offset = usize::try_from(offset).unwrap();
        let end = offset.saturating_add(max_len).min(self.bytes.len());
        let mut chunk = self.bytes[offset..end].to_vec();
        if self.oversized {
            chunk.push(0);
        }
        Ok(chunk)
    }
}

#[derive(Default)]
struct ChunkWriter {
    bytes: Vec<u8>,
}

impl ArtifactWriter for ChunkWriter {
    fn write_chunk(&mut self, offset: u64, bytes: &[u8]) -> UpdateResult<()> {
        assert_eq!(offset as usize, self.bytes.len());
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
}

struct OneFault(UpdateFaultPoint);

impl UpdateFaultInjector for OneFault {
    fn check(&self, point: UpdateFaultPoint) -> UpdateResult<()> {
        if point == self.0 {
            Err(UpdateError::InjectedFault(point))
        } else {
            Ok(())
        }
    }
}

struct MemoryStore {
    active: Mutex<Option<ArtifactDescriptor>>,
}

impl ArtifactStore for MemoryStore {
    fn current(&self) -> UpdateResult<Option<ArtifactDescriptor>> {
        Ok(self.active.lock().unwrap().clone())
    }

    fn stage(
        &self,
        descriptor: &ArtifactDescriptor,
        _bytes: &[u8],
    ) -> UpdateResult<StagedArtifact> {
        Ok(StagedArtifact {
            descriptor: descriptor.clone(),
            staging_reference: "memory".to_string(),
        })
    }

    fn activate(&self, staged: StagedArtifact) -> UpdateResult<ActivationReceipt> {
        let mut active = self.active.lock().unwrap();
        let previous = active.replace(staged.descriptor.clone());
        Ok(ActivationReceipt {
            activated: staged.descriptor,
            previous,
        })
    }

    fn rollback(&self, receipt: &ActivationReceipt) -> UpdateResult<()> {
        *self.active.lock().unwrap() = receipt.previous.clone();
        Ok(())
    }

    fn commit(&self, _receipt: &ActivationReceipt) -> UpdateResult<()> {
        Ok(())
    }
}

fn request() -> UpdateRequest {
    UpdateRequest {
        application_id: ApplicationId::new("app-a").unwrap(),
        current_version: "1.0.0".to_string(),
        channel: "stable".to_string(),
    }
}

fn signed_descriptor(
    version: &str,
    build: &str,
    bytes: &[u8],
    signing_key: &SigningKey,
) -> ArtifactDescriptor {
    let artifact = descriptor(version, build, bytes);
    let signature = signing_key.sign(&artifact_signing_payload(&artifact));
    artifact
        .with_ed25519_signature("release-2026", encode_hex(&signature.to_bytes()))
        .unwrap()
}

fn targeted_signed_descriptor(
    version: &str,
    build: &str,
    bytes: &[u8],
    target: ArtifactTarget,
    signing_key: &SigningKey,
) -> ArtifactDescriptor {
    let artifact = descriptor(version, build, bytes)
        .with_target(target)
        .unwrap();
    let signature = signing_key.sign(&artifact_signing_payload(&artifact));
    artifact
        .with_ed25519_signature("release-2026", encode_hex(&signature.to_bytes()))
        .unwrap()
}

fn signed_file_descriptor(
    version: &str,
    build: &str,
    path: &std::path::Path,
    bytes: &[u8],
    signing_key: &SigningKey,
) -> ArtifactDescriptor {
    let artifact = ArtifactDescriptor::new(
        ApplicationId::new("app-a").unwrap(),
        version,
        BuildId::new(build).unwrap(),
        "stable",
        ">=0.6.0, <1.0.0",
        "1",
        format!("file:{}", path.display()),
        sha256_hex(bytes),
        bytes.len() as u64,
    )
    .unwrap();
    let signature = signing_key.sign(&artifact_signing_payload(&artifact));
    artifact
        .with_ed25519_signature("release-2026", encode_hex(&signature.to_bytes()))
        .unwrap()
}

fn recovery_receipt(
    attempt_id: &str,
    activated: ArtifactDescriptor,
    previous: Option<ArtifactDescriptor>,
) -> ActivationReceiptV2 {
    ActivationReceiptV2 {
        format_version: ACTIVATION_V2_FORMAT_VERSION,
        attempt_id: attempt_id.to_string(),
        phase: ActivationPhaseV2::Prepared,
        activated,
        previous,
        created_at_ms: 100,
        updated_at_ms: 100,
        host_binding: Some("host-attempt".to_string()),
        failure_reason: None,
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn unhealthy_activation_rolls_back_to_previous_artifact() {
    let old = descriptor("1.0.0", "build-old", b"old");
    let new = descriptor("1.1.0", "build-new", b"new");
    let provider = MemoryProvider {
        descriptor: new.clone(),
        bytes: b"new".to_vec(),
    };
    let store = MemoryStore {
        active: Mutex::new(Some(old.clone())),
    };
    let coordinator = UpdateCoordinator::new(&provider, &store, &Healthy(false), 1024).unwrap();
    let outcome = coordinator.apply(&request(), "0.6.1", "1").unwrap();
    assert!(matches!(outcome, UpdateOutcome::RolledBack { .. }));
    assert_eq!(store.current().unwrap(), Some(old));
}

#[test]
fn receive_artifact_streams_bounded_chunks_and_verifies_final_hash() {
    let bytes = b"streamed artifact".to_vec();
    let artifact = descriptor("1.1.0", "streamed", &bytes);
    let source = ChunkSource {
        bytes: bytes.clone(),
        oversized: false,
    };
    let mut writer = ChunkWriter::default();
    receive_artifact(
        &ArtifactTransferPolicy::new(4).unwrap(),
        &artifact,
        &source,
        &mut writer,
    )
    .unwrap();
    assert_eq!(writer.bytes, bytes);
}

#[test]
fn receive_artifact_rejects_oversized_chunks_and_bad_hashes() {
    let bytes = b"streamed artifact".to_vec();
    let artifact = descriptor("1.1.0", "streamed", &bytes);
    let oversized = ChunkSource {
        bytes: bytes.clone(),
        oversized: true,
    };
    let mut writer = ChunkWriter::default();
    assert!(matches!(
        receive_artifact(
            &ArtifactTransferPolicy::new(4).unwrap(),
            &artifact,
            &oversized,
            &mut writer,
        ),
        Err(UpdateError::Transfer(message)) if message.contains("more bytes")
    ));

    let wrong = ArtifactDescriptor::new(
        ApplicationId::new("app-a").unwrap(),
        "1.1.0",
        BuildId::new("wrong").unwrap(),
        "stable",
        ">=0.6.0, <1.0.0",
        "1",
        "memory:wrong",
        sha256_hex(&vec![0_u8; bytes.len()]),
        bytes.len() as u64,
    )
    .unwrap();
    let source = ChunkSource {
        bytes,
        oversized: false,
    };
    let mut writer = ChunkWriter::default();
    assert!(matches!(
        receive_artifact(
            &ArtifactTransferPolicy::new(4).unwrap(),
            &wrong,
            &source,
            &mut writer,
        ),
        Err(UpdateError::ChecksumMismatch)
    ));
}

#[test]
fn update_cache_publishes_and_reuses_a_verified_hash_addressed_artifact() {
    let root = std::fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!("appcore-update-cache-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let source_path = root.join("source.bin");
    fs::create_dir_all(&root).unwrap();
    let bytes = b"cache artifact";
    fs::write(&source_path, bytes).unwrap();
    let signing_key = SigningKey::from_bytes(&[9_u8; 32]);
    let descriptor =
        signed_file_descriptor("1.1.0", "cache-build", &source_path, bytes, &signing_key);
    let mut verifier = Ed25519ArtifactVerifier::new();
    verifier
        .add_trust_root("release-2026", signing_key.verifying_key().to_bytes())
        .unwrap();
    let cache_root = root.join("cache");
    let cache = UpdateCache::open(&cache_root, CacheOptions::new(1024, true).unwrap()).unwrap();
    let first = cache
        .stage(
            &verifier,
            &descriptor,
            &FileArtifactSource,
            &ArtifactTransferPolicy::new(4).unwrap(),
        )
        .unwrap();
    assert_eq!(fs::read(&first.artifact_path).unwrap(), bytes);
    assert!(!cache.partial_path(&descriptor).exists());
    let second = cache
        .stage(
            &verifier,
            &descriptor,
            &FileArtifactSource,
            &ArtifactTransferPolicy::new(4).unwrap(),
        )
        .unwrap();
    assert_eq!(first, second);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn update_cache_resumes_a_partial_artifact_and_preserves_existing_entries_on_quota() {
    let root = std::env::temp_dir().join(format!(
        "appcore-update-cache-resume-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let source_path = root.join("source.bin");
    let bytes = b"cache resume artifact";
    fs::write(&source_path, bytes).unwrap();
    let signing_key = SigningKey::from_bytes(&[10_u8; 32]);
    let descriptor =
        signed_file_descriptor("1.1.0", "resume-build", &source_path, bytes, &signing_key);
    let mut verifier = Ed25519ArtifactVerifier::new();
    verifier
        .add_trust_root("release-2026", signing_key.verifying_key().to_bytes())
        .unwrap();
    let cache = UpdateCache::open(
        root.join("cache"),
        CacheOptions::new(bytes.len() as u64 + 1024, false).unwrap(),
    )
    .unwrap();
    fs::write(cache.partial_path(&descriptor), b"xxxxx").unwrap();
    cache
        .stage(
            &verifier,
            &descriptor,
            &FileArtifactSource,
            &ArtifactTransferPolicy::new(4).unwrap(),
        )
        .unwrap();
    assert_eq!(fs::read(cache.artifact_path(&descriptor)).unwrap(), bytes);
    let protected = cache.artifact_path(&descriptor);
    let too_small = UpdateCache::open(
        root.join("small-cache"),
        CacheOptions::new(1, false).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        too_small.stage(
            &verifier,
            &descriptor,
            &FileArtifactSource,
            &ArtifactTransferPolicy::new(4).unwrap(),
        ),
        Err(UpdateError::Store(message)) if message.contains("quota")
    ));
    assert!(protected.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn quarantine_is_bounded_persistent_and_requires_explicit_release() {
    let root =
        std::env::temp_dir().join(format!("appcore-update-quarantine-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let first = descriptor("1.1.0", "quarantine-one", b"one");
    let second = descriptor("1.2.0", "quarantine-two", b"two");
    let store = QuarantineStore::open(&root, 2).unwrap();
    let record = store
        .quarantine(
            &first,
            QuarantineReason::HealthCheckFailed("health endpoint failed".to_string()),
            100,
        )
        .unwrap();
    assert_eq!(record.state, QuarantineState::Active);
    assert!(store
        .is_quarantined(&QuarantineKey::from_descriptor(&first))
        .unwrap());

    let reopened = QuarantineStore::open(&root, 2).unwrap();
    assert_eq!(reopened.list().unwrap().len(), 1);
    assert!(reopened
        .release(&QuarantineKey::from_descriptor(&first), 200)
        .unwrap());
    assert!(!reopened
        .is_quarantined(&QuarantineKey::from_descriptor(&first))
        .unwrap());
    assert!(!reopened
        .release(&QuarantineKey::from_descriptor(&first), 300)
        .unwrap());

    reopened
        .quarantine(
            &first,
            QuarantineReason::ManualReview("retry approved".to_string()),
            400,
        )
        .unwrap();
    assert!(reopened
        .is_quarantined(&QuarantineKey::from_descriptor(&first))
        .unwrap());
    reopened
        .quarantine(&second, QuarantineReason::ChecksumMismatch, 500)
        .unwrap();
    let third = descriptor("1.3.0", "quarantine-three", b"three");
    assert!(matches!(
        reopened.quarantine(&third, QuarantineReason::ActivationFailed("failed".to_string()), 600),
        Err(UpdateError::Recovery(message)) if message.contains("bound")
    ));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn release_catalog_selects_highest_matching_platform_version() {
    let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
    let mut verifier = Ed25519ArtifactVerifier::new();
    verifier
        .add_trust_root("release-2026", signing_key.verifying_key().to_bytes())
        .unwrap();
    let mac_arm = ArtifactTarget::new("macos", "aarch64", "tauri").unwrap();
    let linux_x86 = ArtifactTarget::new("linux", "x86_64", "raw").unwrap();
    let catalog = ReleaseCatalog::from_entries(
        vec![
            targeted_signed_descriptor("1.1.0", "mac-old", b"old", mac_arm.clone(), &signing_key),
            targeted_signed_descriptor("1.3.0", "mac-new", b"new", mac_arm.clone(), &signing_key),
            targeted_signed_descriptor("1.9.0", "linux", b"linux", linux_x86, &signing_key),
        ],
        &verifier,
    )
    .unwrap();
    let selected = catalog
        .select(&UpdateIdentity {
            application_id: ApplicationId::new("app-a").unwrap(),
            os: "macos".to_string(),
            architecture: "aarch64".to_string(),
            format: "tauri".to_string(),
            channel: "stable".to_string(),
            current_version: "1.0.0".to_string(),
        })
        .unwrap()
        .unwrap();
    assert_eq!(selected.application_version(), "1.3.0");
    assert_eq!(selected.build_id().as_str(), "mac-new");
}

#[test]
fn release_catalog_rejects_invalid_signature_and_ambiguous_duplicates() {
    let signing_key = SigningKey::from_bytes(&[8_u8; 32]);
    let mut verifier = Ed25519ArtifactVerifier::new();
    verifier
        .add_trust_root("release-2026", signing_key.verifying_key().to_bytes())
        .unwrap();
    let target = ArtifactTarget::new("macos", "aarch64", "tauri").unwrap();
    let valid = targeted_signed_descriptor("1.1.0", "build", b"new", target, &signing_key);
    let invalid = valid
        .clone()
        .with_ed25519_signature("release-2026", "00".repeat(64))
        .unwrap();
    assert!(matches!(
        ReleaseCatalog::from_entries(vec![invalid], &verifier),
        Err(UpdateError::Authenticity(_))
    ));
    assert!(matches!(
        ReleaseCatalog::from_entries(vec![valid.clone(), valid], &verifier),
        Err(UpdateError::InvalidArtifact(message)) if message.contains("ambiguous")
    ));
}

#[test]
fn peer_update_payloads_are_path_free_and_validate_stream_bounds() {
    let digest = sha256_hex(b"artifact");
    let offer = ArtifactOfferRequestV2 {
        application_id: ApplicationId::new("app-a").unwrap(),
        build_id: BuildId::new("peer-build").unwrap(),
        target: ArtifactTarget::new("linux", "x86_64", "raw").unwrap(),
        sha256: digest.clone(),
        size_bytes: 7,
        protocol_version: "1".to_string(),
    };
    offer.validate().unwrap();
    let encoded = serde_json::to_string(&offer).unwrap();
    assert!(!encoded.contains("artifact_reference"));
    assert!(!encoded.contains("file:"));

    let response = ArtifactOfferResponseV2 {
        status: ArtifactPeerStatusV2::Accepted,
        sha256: digest.clone(),
        size_bytes: 7,
        max_chunk_bytes: 4,
        protocol_version: "1".to_string(),
    };
    response.validate_against(&offer).unwrap();
    let request = ArtifactChunkRequestV2 {
        sha256: digest.clone(),
        offset: 3,
        len: 4,
    };
    request
        .validate_against(&offer, response.max_chunk_bytes)
        .unwrap();
    let chunk = ArtifactChunkResponseV2 {
        sha256: digest,
        offset: 3,
        len: 4,
        chunk_sha256: sha256_hex(b"tifact"),
        total_size_bytes: 7,
    };
    chunk.validate_against(&request, offer.size_bytes).unwrap();
}

#[test]
fn peer_update_payloads_reject_wrong_identity_and_out_of_range_chunks() {
    let offer = ArtifactOfferRequestV2 {
        application_id: ApplicationId::new("app-a").unwrap(),
        build_id: BuildId::new("peer-build").unwrap(),
        target: ArtifactTarget::new("linux", "x86_64", "raw").unwrap(),
        sha256: sha256_hex(b"artifact"),
        size_bytes: 7,
        protocol_version: "1".to_string(),
    };
    let request = ArtifactChunkRequestV2 {
        sha256: sha256_hex(b"different"),
        offset: 6,
        len: 4,
    };
    assert!(matches!(
        request.validate_against(&offer, 4),
        Err(UpdateError::Transfer(message)) if message.contains("bounds")
    ));
    let response = ArtifactOfferResponseV2 {
        status: ArtifactPeerStatusV2::Accepted,
        sha256: sha256_hex(b"different"),
        size_bytes: 7,
        max_chunk_bytes: 4,
        protocol_version: "1".to_string(),
    };
    assert!(response.validate_against(&offer).is_err());
}

#[test]
fn v2_recovery_inspection_and_commit_replay_are_fenced_and_idempotent() {
    let root =
        std::env::temp_dir().join(format!("appcore-update-recovery-v2-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let old = descriptor("1.0.0", "old", b"old");
    let new = descriptor("1.1.0", "new", b"new");
    let store = FileRecoveryStore::open(&root).unwrap();
    store
        .prepare(recovery_receipt("attempt-a", new.clone(), Some(old)))
        .unwrap();
    assert!(matches!(
        store.inspect_recovery().unwrap(),
        RecoveryDecision::PendingActivation { receipt }
            if receipt.phase == ActivationPhaseV2::Prepared
    ));
    store.mark_activated("attempt-a", 200).unwrap();
    assert!(matches!(
        store.replay(RecoveryAction::ConfirmHealthy {
            attempt_id: "wrong".to_string(),
            activated_sha256: new.sha256().to_string(),
            at_ms: 300,
        }),
        Err(UpdateError::Recovery(message)) if message.contains("another attempt")
    ));
    let result = store
        .replay(RecoveryAction::ConfirmHealthy {
            attempt_id: "attempt-a".to_string(),
            activated_sha256: new.sha256().to_string(),
            at_ms: 300,
        })
        .unwrap();
    assert_eq!(result.phase, ActivationPhaseV2::Committed);
    let repeated = store
        .replay(RecoveryAction::ConfirmHealthy {
            attempt_id: "attempt-a".to_string(),
            activated_sha256: new.sha256().to_string(),
            at_ms: 301,
        })
        .unwrap();
    assert!(repeated.idempotent);
    assert!(matches!(
        store.inspect_recovery().unwrap(),
        RecoveryDecision::Committed { .. }
    ));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn v2_recovery_requires_explicit_rollback_evidence_and_rejects_unknown_format() {
    let root = std::env::temp_dir().join(format!(
        "appcore-update-recovery-rollback-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let old = descriptor("1.0.0", "old", b"old");
    let new = descriptor("1.1.0", "new", b"new");
    let store = FileRecoveryStore::open(&root).unwrap();
    store
        .prepare(recovery_receipt("attempt-b", new, Some(old.clone())))
        .unwrap();
    store.mark_activated("attempt-b", 200).unwrap();
    store
        .replay(RecoveryAction::RequireRollback {
            attempt_id: "attempt-b".to_string(),
            reason: "health failed".to_string(),
            at_ms: 300,
        })
        .unwrap();
    assert!(matches!(
        store.replay(RecoveryAction::RecordRolledBack {
            attempt_id: "attempt-b".to_string(),
            active_sha256: Some(sha256_hex(b"wrong")),
            at_ms: 400,
        }),
        Err(UpdateError::Recovery(message)) if message.contains("conflicts")
    ));
    store
        .replay(RecoveryAction::RecordRolledBack {
            attempt_id: "attempt-b".to_string(),
            active_sha256: Some(old.sha256().to_string()),
            at_ms: 401,
        })
        .unwrap();
    let decision = store.inspect_recovery().unwrap();
    assert!(
        matches!(decision, RecoveryDecision::RolledBack { .. }),
        "{decision:?}"
    );
    fs::write(root.join("activation-v2.json"), br#"{"format_version":99}"#).unwrap();
    assert!(matches!(
        store.inspect_recovery(),
        Err(UpdateError::Recovery(message)) if message.contains("NO MORE SUPPORTED")
    ));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn fault_after_activation_is_rolled_back() {
    let old = descriptor("1.0.0", "build-old", b"old");
    let new = descriptor("1.1.0", "build-new", b"new");
    let provider = MemoryProvider {
        descriptor: new,
        bytes: b"new".to_vec(),
    };
    let store = MemoryStore {
        active: Mutex::new(Some(old.clone())),
    };
    let coordinator = UpdateCoordinator::new(&provider, &store, &Healthy(true), 1024).unwrap();
    let outcome = coordinator
        .apply_with_faults(
            &request(),
            "0.6.1",
            "1",
            &OneFault(UpdateFaultPoint::AfterActivation),
        )
        .unwrap();
    assert!(matches!(outcome, UpdateOutcome::RolledBack { .. }));
    assert_eq!(store.current().unwrap(), Some(old));
}

#[test]
fn file_store_activates_commits_and_rolls_back() {
    let root = std::env::temp_dir().join(format!(
        "appcore-update-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let _ = fs::remove_dir_all(&root);
    let store = FileArtifactStore::new(&root);
    let old = descriptor("1.0.0", "build-old", b"old");
    let old_staged = store.stage(&old, b"old").unwrap();
    let old_receipt = store.activate(old_staged).unwrap();
    store.commit(&old_receipt).unwrap();
    let new = descriptor("1.1.0", "build-new", b"new");
    let new_receipt = store.activate(store.stage(&new, b"new").unwrap()).unwrap();
    assert_eq!(store.current().unwrap(), Some(new));
    store.rollback(&new_receipt).unwrap();
    assert_eq!(store.current().unwrap(), Some(old));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn file_store_revalidates_staged_bytes_before_activation() {
    let root = std::env::temp_dir().join(format!(
        "appcore-update-staged-revalidation-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let store = FileArtifactStore::new(&root);
    let candidate = descriptor("1.1.0", "build-candidate", b"candidate");
    let staged = store.stage(&candidate, b"candidate").unwrap();
    fs::write(store.staged_artifact_path(&staged), b"tampered!").unwrap();

    assert!(matches!(
        store.activate(staged),
        Err(UpdateError::ChecksumMismatch)
    ));
    assert!(store.current().unwrap().is_none());
    assert!(!store.artifact_path(candidate.build_id()).exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn file_store_streams_large_staged_artifact_activation() {
    let root = std::env::temp_dir().join(format!(
        "appcore-update-streaming-activation-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let store = FileArtifactStore::new(&root);
    let bytes = (0..16 * 1024 * 1024)
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    let candidate = descriptor("1.1.0", "build-large", &bytes);
    let staged = store.stage(&candidate, &bytes).unwrap();
    drop(bytes);

    let receipt = store.activate(staged).unwrap();

    assert_eq!(store.current().unwrap(), Some(candidate));
    assert_eq!(
        fs::metadata(store.artifact_path(receipt.activated.build_id()))
            .unwrap()
            .len(),
        16 * 1024 * 1024
    );
    store.commit(&receipt).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn file_store_rejects_staged_artifact_size_changes() {
    let root =
        std::env::temp_dir().join(format!("appcore-update-staged-size-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let store = FileArtifactStore::new(&root);
    let candidate = descriptor("1.1.0", "build-size-change", b"candidate");
    let staged = store.stage(&candidate, b"candidate").unwrap();
    let path = store.staged_artifact_path(&staged);
    fs::OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(b"extra")
        .unwrap();

    assert!(matches!(
        store.activate(staged),
        Err(UpdateError::ChecksumMismatch)
    ));
    assert!(store.current().unwrap().is_none());
    assert!(!store.artifact_path(candidate.build_id()).exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn file_store_never_replaces_an_existing_build_artifact() {
    let root = std::env::temp_dir().join(format!(
        "appcore-update-immutable-artifact-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let store = FileArtifactStore::new(&root);
    let original = descriptor("1.0.0", "reused-build", b"original");
    let receipt = store
        .activate(store.stage(&original, b"original").unwrap())
        .unwrap();
    store.commit(&receipt).unwrap();
    let replacement = descriptor("1.1.0", "reused-build", b"replacement");
    let staged = store.stage(&replacement, b"replacement").unwrap();

    assert!(matches!(
        store.activate(staged),
        Err(UpdateError::ChecksumMismatch)
    ));
    assert_eq!(
        fs::read(store.artifact_path(original.build_id())).unwrap(),
        b"original"
    );
    assert_eq!(store.current().unwrap(), Some(original));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn file_store_recovers_interrupted_activation_by_rolling_back() {
    let root = std::env::temp_dir().join(format!(
        "appcore-update-recovery-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let _ = fs::remove_dir_all(&root);
    let store = FileArtifactStore::new(&root);
    let old = descriptor("1.0.0", "build-old", b"old");
    let old_receipt = store.activate(store.stage(&old, b"old").unwrap()).unwrap();
    store.commit(&old_receipt).unwrap();
    let new = descriptor("1.1.0", "build-new", b"new");
    let _interrupted = store.activate(store.stage(&new, b"new").unwrap()).unwrap();

    FileArtifactStore::new(&root).recover().unwrap();

    assert_eq!(store.current().unwrap(), Some(old));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unversioned_pending_activation_is_rejected_with_upgrade_wall() {
    let root = std::env::temp_dir().join(format!(
        "appcore-update-metadata-rejection-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let store = FileArtifactStore::new(&root);
    let candidate = descriptor("1.1.0", "build-rejected", b"candidate");
    let receipt = store
        .activate(store.stage(&candidate, b"candidate").unwrap())
        .unwrap();
    fs::write(
        root.join("pending-activation.json"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .unwrap();

    assert!(matches!(
        store.pending_activation_receipt(),
        Err(UpdateError::Store(message)) if message == "NO MORE SUPPORTED PLEASE UPDATE"
    ));
    store.rollback(&receipt).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn update_metadata_rejects_future_format() {
    let root = std::env::temp_dir().join(format!(
        "appcore-update-metadata-future-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let active = descriptor("1.0.0", "build-future", b"active");
    fs::write(
        root.join("active.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "format_version": 2,
            "descriptor": active
        }))
        .unwrap(),
    )
    .unwrap();

    assert!(FileArtifactStore::new(&root).current().is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn coordinator_rejects_downgrade_from_untrusted_provider_selection() {
    let old = descriptor("1.1.0", "build-active", b"active");
    let downgrade = descriptor("1.0.0", "build-downgrade", b"old");
    let provider = MemoryProvider {
        descriptor: downgrade,
        bytes: b"old".to_vec(),
    };
    let store = MemoryStore {
        active: Mutex::new(Some(old)),
    };
    let coordinator = UpdateCoordinator::new(&provider, &store, &Healthy(true), 1024).unwrap();

    assert!(matches!(
        coordinator.apply(&request(), "0.6.1", "1"),
        Err(UpdateError::Incompatible(_))
    ));
}

#[test]
fn coordinator_rejects_reused_active_build_identity() {
    let active = descriptor("1.0.0", "build-reused", b"active");
    let reused = descriptor("1.1.0", "build-reused", b"new");
    let provider = MemoryProvider {
        descriptor: reused,
        bytes: b"new".to_vec(),
    };
    let store = MemoryStore {
        active: Mutex::new(Some(active)),
    };
    let coordinator = UpdateCoordinator::new(&provider, &store, &Healthy(true), 1024).unwrap();

    assert!(matches!(
        coordinator.apply(&request(), "0.6.1", "1"),
        Err(UpdateError::Incompatible(_))
    ));
}

#[test]
fn prepare_leaves_activation_pending_for_application_parent() {
    let root = std::env::temp_dir().join(format!(
        "appcore-update-prepare-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let _ = fs::remove_dir_all(&root);
    let store = FileArtifactStore::new(&root);
    let candidate = descriptor("1.1.0", "build-prepared", b"prepared");
    let provider = MemoryProvider {
        descriptor: candidate.clone(),
        bytes: b"prepared".to_vec(),
    };
    let coordinator = UpdateCoordinator::new(&provider, &store, &Healthy(true), 1024).unwrap();

    assert_eq!(
        coordinator.prepare(&request(), "0.6.1", "1").unwrap(),
        UpdatePreparation::AwaitingHealth(Box::new(candidate.clone()))
    );
    assert_eq!(store.current().unwrap(), Some(candidate));
    assert!(store.pending_activation_receipt().unwrap().is_some());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn checksum_mismatch_never_reaches_activation() {
    let artifact = descriptor("1.1.0", "build-new", b"expected");
    let provider = MemoryProvider {
        descriptor: artifact,
        bytes: b"modified".to_vec(),
    };
    let store = MemoryStore {
        active: Mutex::new(None),
    };
    let coordinator = UpdateCoordinator::new(&provider, &store, &Healthy(true), 1024).unwrap();
    assert!(matches!(
        coordinator.apply(&request(), "0.6.1", "1"),
        Err(UpdateError::ArtifactTooLarge { .. }) | Err(UpdateError::ChecksumMismatch)
    ));
    assert!(store.current().unwrap().is_none());
}

#[test]
fn trusted_signature_allows_activation() {
    let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
    let artifact = signed_descriptor("1.1.0", "build-signed", b"signed", &signing_key);
    let provider = MemoryProvider {
        descriptor: artifact.clone(),
        bytes: b"signed".to_vec(),
    };
    let store = MemoryStore {
        active: Mutex::new(None),
    };
    let mut verifier = Ed25519ArtifactVerifier::new();
    verifier
        .add_trust_root("release-2026", signing_key.verifying_key().to_bytes())
        .unwrap();
    let coordinator = UpdateCoordinator::new_with_authenticity(
        &provider,
        &store,
        &Healthy(true),
        &verifier,
        1024,
    )
    .unwrap();

    assert_eq!(
        coordinator.apply(&request(), "0.6.1", "1").unwrap(),
        UpdateOutcome::Applied(artifact)
    );
}

#[test]
fn tampered_signed_descriptor_never_reaches_activation() {
    let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
    let signed = signed_descriptor("1.1.0", "build-signed", b"signed", &signing_key);
    let tampered = ArtifactDescriptor::new(
        signed.application_id().clone(),
        "1.2.0",
        signed.build_id().clone(),
        signed.channel(),
        signed.runtime_requirement(),
        signed.protocol_version(),
        signed.artifact_reference(),
        signed.sha256(),
        signed.size_bytes(),
    )
    .unwrap()
    .with_ed25519_signature(
        signed.signing_key_id().unwrap(),
        signed.ed25519_signature().unwrap(),
    )
    .unwrap();
    let provider = MemoryProvider {
        descriptor: tampered,
        bytes: b"signed".to_vec(),
    };
    let store = MemoryStore {
        active: Mutex::new(None),
    };
    let mut verifier = Ed25519ArtifactVerifier::new();
    verifier
        .add_trust_root("release-2026", signing_key.verifying_key().to_bytes())
        .unwrap();
    let coordinator = UpdateCoordinator::new_with_authenticity(
        &provider,
        &store,
        &Healthy(true),
        &verifier,
        1024,
    )
    .unwrap();

    assert!(matches!(
        coordinator.apply(&request(), "0.6.1", "1"),
        Err(UpdateError::Authenticity(_))
    ));
    assert!(store.current().unwrap().is_none());
}

#[test]
fn signing_key_rotation_accepts_deprecated_and_rejects_revoked_keys() {
    let signing_key = SigningKey::from_bytes(&[9_u8; 32]);
    let artifact = signed_descriptor("1.1.0", "build-rotated", b"signed", &signing_key);
    let mut verifier = Ed25519ArtifactVerifier::new();
    verifier
        .add_trust_root("release-2026", signing_key.verifying_key().to_bytes())
        .unwrap();
    verifier
        .set_trust_root_status("release-2026", SigningKeyStatus::Deprecated)
        .unwrap();
    assert!(verifier.verify(&artifact).is_ok());

    verifier
        .set_trust_root_status("release-2026", SigningKeyStatus::Revoked)
        .unwrap();
    assert!(matches!(
        verifier.verify(&artifact),
        Err(UpdateError::Authenticity(_))
    ));
}

#[test]
fn artifact_policy_enforces_exact_channel_and_origin() {
    let bytes = b"signed";
    let artifact = ArtifactDescriptor::new(
        ApplicationId::new("app-a").unwrap(),
        "1.1.0",
        BuildId::new("build-policy").unwrap(),
        "stable",
        ">=0.6.0, <1.0.0",
        "1",
        "https://updates.example/artifacts/app-a",
        sha256_hex(bytes),
        bytes.len() as u64,
    )
    .unwrap();
    let policy = ArtifactTrustPolicy::new()
        .allow_channel("stable")
        .unwrap()
        .allow_origin("https://updates.example")
        .unwrap();
    assert!(policy.verify(&artifact).is_ok());

    let wrong_origin = ArtifactTrustPolicy::new()
        .allow_channel("stable")
        .unwrap()
        .allow_origin("https://mirror.example")
        .unwrap();
    assert!(matches!(
        wrong_origin.verify(&artifact),
        Err(UpdateError::Authenticity(_))
    ));
}

#[test]
fn every_pre_commit_fault_preserves_or_restores_the_previous_artifact() {
    for point in [
        UpdateFaultPoint::AfterSelection,
        UpdateFaultPoint::AfterVerification,
        UpdateFaultPoint::AfterStaging,
        UpdateFaultPoint::AfterActivation,
        UpdateFaultPoint::BeforeCommit,
    ] {
        let old = descriptor("1.0.0", "build-old", b"old");
        let new = descriptor("1.1.0", "build-new", b"new");
        let provider = MemoryProvider {
            descriptor: new,
            bytes: b"new".to_vec(),
        };
        let store = MemoryStore {
            active: Mutex::new(Some(old.clone())),
        };
        let coordinator = UpdateCoordinator::new(&provider, &store, &Healthy(true), 1024).unwrap();
        let result = coordinator.apply_with_faults(&request(), "0.6.1", "1", &OneFault(point));
        match point {
            UpdateFaultPoint::AfterSelection
            | UpdateFaultPoint::AfterVerification
            | UpdateFaultPoint::AfterStaging => {
                assert!(matches!(result, Err(UpdateError::InjectedFault(_))));
            }
            UpdateFaultPoint::AfterActivation | UpdateFaultPoint::BeforeCommit => {
                assert!(matches!(result, Ok(UpdateOutcome::RolledBack { .. })));
            }
        }
        assert_eq!(store.current().unwrap(), Some(old));
    }
}

#[test]
fn file_store_recovers_every_internal_activation_phase() {
    for point in [
        StoreFaultPoint::ArtifactMoved,
        StoreFaultPoint::PreviousPointerWritten,
        StoreFaultPoint::PendingReceiptWritten,
        StoreFaultPoint::ActivePointerWritten,
    ] {
        let root = std::env::temp_dir().join(format!(
            "appcore-update-store-fault-{point:?}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let store = FileArtifactStore::new(&root);
        let old = descriptor("1.0.0", "build-old", b"old");
        let old_receipt = store.activate(store.stage(&old, b"old").unwrap()).unwrap();
        store.commit(&old_receipt).unwrap();
        let new = descriptor("1.1.0", "build-new", b"new");
        let staged = store.stage(&new, b"new").unwrap();

        assert!(store.activate_with_fault(staged, point).is_err());
        let recovered = FileArtifactStore::new(&root);
        recovered.recover().unwrap();

        assert_eq!(recovered.current().unwrap(), Some(old));
        assert!(recovered.pending_activation_receipt().unwrap().is_none());
        assert!(!root.join("previous.json").exists());
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn file_update_provider_and_store_reject_symlinks() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!("appcore-update-symlink-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let candidate = descriptor("1.1.0", "build-symlink", b"candidate");
    let outside = root.join("outside-index.json");
    fs::write(
        &outside,
        serde_json::to_vec(&vec![candidate.clone()]).unwrap(),
    )
    .unwrap();
    let index = root.join("index.json");
    symlink(&outside, &index).unwrap();
    let provider = FileUpdateProvider::new(&index);
    assert!(provider.latest(&request()).is_err());

    let store_root = root.join("store");
    fs::create_dir_all(&store_root).unwrap();
    let outside_pointer = root.join("outside-pointer.json");
    fs::write(
        &outside_pointer,
        serde_json::to_vec(&serde_json::json!({
            "format_version": UPDATE_METADATA_FORMAT_VERSION,
            "descriptor": candidate
        }))
        .unwrap(),
    )
    .unwrap();
    symlink(&outside_pointer, store_root.join("active.json")).unwrap();
    assert!(FileArtifactStore::new(&store_root).current().is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn file_update_provider_selects_latest_candidate_in_index_order() {
    let root = std::env::temp_dir().join(format!(
        "appcore-update-provider-selection-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let other_application = ArtifactDescriptor::new(
        ApplicationId::new("app-b").unwrap(),
        "9.0.0",
        BuildId::new("build-other-app").unwrap(),
        "stable",
        ">=0.6.0, <1.0.0",
        "1",
        "memory:build-other-app",
        sha256_hex(b"other-app"),
        9,
    )
    .unwrap();
    let other_channel = ArtifactDescriptor::new(
        ApplicationId::new("app-a").unwrap(),
        "8.0.0",
        BuildId::new("build-other-channel").unwrap(),
        "beta",
        ">=0.6.0, <1.0.0",
        "1",
        "memory:build-other-channel",
        sha256_hex(b"other-channel"),
        13,
    )
    .unwrap();
    let artifacts = vec![
        descriptor("1.0.0", "build-current", b"current"),
        other_application,
        descriptor("3.0.0", "build-first-highest", b"first-highest"),
        descriptor("2.5.0", "build-lower", b"lower"),
        descriptor("3.0.0", "build-equal-later", b"equal-later"),
        other_channel,
    ];
    let index = root.join("index.json");
    fs::write(&index, serde_json::to_vec(&artifacts).unwrap()).unwrap();
    let provider = FileUpdateProvider::new(index);

    let selected = provider.latest(&request()).unwrap().unwrap();
    assert_eq!(selected.build_id().as_str(), "build-first-highest");

    let mut current_is_latest = request();
    current_is_latest.current_version = "3.0.0".to_string();
    assert!(provider.latest(&current_is_latest).unwrap().is_none());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn file_update_index_decoder_bounds_streaming_input() {
    let artifact = descriptor("1.1.0", "build-index-limit", b"index-limit");
    let mut encoded = serde_json::to_vec(&vec![artifact.clone()]).unwrap();
    encoded.resize(crate::provider::FILE_UPDATE_INDEX_MAX_BYTES, b' ');
    let declared_length = u64::try_from(encoded.len()).unwrap();
    let request = request();
    let current = semver::Version::parse(&request.current_version).unwrap();

    let selected =
        crate::provider::select_index(Cursor::new(&encoded), declared_length, &request, &current)
            .unwrap();
    assert_eq!(selected, Some(artifact));

    encoded.push(b' ');
    let error =
        crate::provider::select_index(Cursor::new(encoded), declared_length, &request, &current)
            .unwrap_err();
    assert!(matches!(error, UpdateError::Provider(message) if message.contains("read limit")));

    let malformed =
        crate::provider::select_index(Cursor::new(b"["), 1, &request, &current).unwrap_err();
    assert!(matches!(malformed, UpdateError::Provider(_)));
}

#[test]
fn file_update_index_stream_preserves_artifact_validation_errors() {
    let artifact = descriptor("1.1.0", "build-invalid-index", b"invalid-index");
    let mut encoded = serde_json::to_value(vec![artifact]).unwrap();
    encoded[0]["application_version"] = serde_json::json!("invalid-version");
    let encoded = serde_json::to_vec(&encoded).unwrap();
    let request = request();
    let current = semver::Version::parse(&request.current_version).unwrap();

    let error = crate::provider::select_index(
        Cursor::new(&encoded),
        u64::try_from(encoded.len()).unwrap(),
        &request,
        &current,
    )
    .unwrap_err();
    assert!(
        matches!(error, UpdateError::InvalidArtifact(message) if message.contains("application version"))
    );
}

#[test]
fn file_update_index_declared_oversize_is_rejected_before_read() {
    struct PanicReader;

    impl Read for PanicReader {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            panic!("oversized update index must not be read");
        }
    }

    let declared_length = u64::try_from(crate::provider::FILE_UPDATE_INDEX_MAX_BYTES)
        .unwrap()
        .saturating_add(1);
    let request = request();
    let current = semver::Version::parse(&request.current_version).unwrap();
    let error = crate::provider::select_index(PanicReader, declared_length, &request, &current)
        .unwrap_err();
    assert!(matches!(error, UpdateError::Provider(message) if message.contains("read limit")));
}

#[cfg(all(feature = "allow-unsigned-local-artifacts", unix))]
#[test]
fn unsigned_local_verifier_is_owner_only_and_confined_to_canonical_root() {
    use std::os::unix::fs::{symlink, PermissionsExt};

    let root = std::env::temp_dir().join(format!("appcore-unsigned-update-{}", std::process::id()));
    let outside =
        std::env::temp_dir().join(format!("appcore-unsigned-outside-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_file(&outside);
    fs::create_dir_all(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let root = fs::canonicalize(root).unwrap();
    let artifact_path = root.join("candidate.bin");
    fs::write(&artifact_path, b"candidate").unwrap();
    fs::set_permissions(&artifact_path, fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(&outside, b"outside").unwrap();
    fs::set_permissions(&outside, fs::Permissions::from_mode(0o600)).unwrap();
    let outside = fs::canonicalize(outside).unwrap();

    let verifier = UnsignedLocalArtifactVerifier::new(&root).unwrap();
    assert!(verifier
        .verify(&file_descriptor("inside", &artifact_path, b"candidate"))
        .is_ok());
    assert!(verifier
        .verify(&file_descriptor("outside", &outside, b"outside"))
        .is_err());

    let link = root.join("candidate-link");
    symlink(&artifact_path, &link).unwrap();
    assert!(verifier
        .verify(&file_descriptor("link", &link, b"candidate"))
        .is_err());

    fs::remove_dir_all(root).unwrap();
    fs::remove_file(outside).unwrap();
}

#[cfg(all(feature = "allow-unsigned-local-artifacts", unix))]
fn file_descriptor(build: &str, path: &std::path::Path, bytes: &[u8]) -> ArtifactDescriptor {
    ArtifactDescriptor::new(
        ApplicationId::new("app-a").unwrap(),
        "1.1.0",
        BuildId::new(build).unwrap(),
        "stable",
        ">=1.0.0",
        "1",
        format!("file:{}", path.display()),
        sha256_hex(bytes),
        bytes.len() as u64,
    )
    .unwrap()
}
