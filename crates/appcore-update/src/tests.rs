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
