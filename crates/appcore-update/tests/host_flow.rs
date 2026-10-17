// =============================================================================
//        #######
//     ###       ###     F: host_flow.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/12 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/12 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Host-neutral failure/restart qualification using durable V2 receipts.
use appcore_contracts::{ApplicationId, BuildId};
use appcore_update::*;
use std::sync::Mutex;
struct Host {
    calls: Mutex<Vec<&'static str>>,
    failure: Mutex<Option<&'static str>>,
    installed: std::path::PathBuf,
}
impl Host {
    fn call(&self, phase: &'static str) -> UpdateResult<()> {
        self.calls.lock().unwrap().push(phase);
        if *self.failure.lock().unwrap() == Some(phase) {
            return Err(UpdateError::Recovery("injected host failure".into()));
        }
        Ok(())
    }
}
impl UpdateRecoveryGate for Host {
    fn drain_and_snapshot(&self, _: &ActivationRequest) -> UpdateResult<()> {
        self.call("snapshot")
    }
    fn seal(&self, _: &ActivationRequest) -> UpdateResult<()> {
        self.call("seal")
    }
}
impl ActivationAdapter for Host {
    fn prepare(&self, _: &ActivationRequest, _: &StagedArtifact) -> UpdateResult<()> {
        self.call("prepare")
    }
    fn activate(
        &self,
        request: &ActivationRequest,
        _: &StagedArtifact,
    ) -> UpdateResult<ActivationEvidence> {
        self.call("activate")?;
        let evidence = ActivationEvidence {
            attempt_id: request.attempt_id.clone(),
            descriptor: request.descriptor.clone(),
            host_binding: request.host_binding.clone(),
        };
        let observation = evidence.clone().into_receipt(request.previous.clone(), 0)?;
        std::fs::write(&self.installed, serde_json::to_vec(&observation).unwrap())
            .map_err(|_| UpdateError::Store("fixture host observation failed".into()))?;
        self.call("activate-reply")?;
        Ok(evidence)
    }
    fn healthcheck(&self, _: &ActivationEvidence) -> UpdateResult<()> {
        self.call("health")
    }
    fn commit(&self, _: &ActivationReceiptV2) -> UpdateResult<()> {
        self.call("commit")
    }
    fn rollback(&self, _: &ActivationReceiptV2) -> UpdateResult<()> {
        self.call("rollback")
    }
    fn recover(&self, _: &RecoveryDecision) -> UpdateResult<RecoveryAction> {
        Err(UpdateError::Recovery("host inspection required".into()))
    }
}
struct Fixture {
    root: std::path::PathBuf,
    journal: FileRecoveryStore,
    host: Host,
    request: ActivationRequest,
    staged: StagedArtifact,
}
impl Fixture {
    fn new(label: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("appcore-host-flow-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let journal = FileRecoveryStore::open(&root).unwrap();
        let descriptor = ArtifactDescriptor::new(
            ApplicationId::new("app-a").unwrap(),
            "1.1.0",
            BuildId::new("build-b").unwrap(),
            "stable",
            ">=1.0.0",
            "1",
            "memory:artifact",
            "11".repeat(32),
            1,
        )
        .unwrap();
        let request = ActivationRequest {
            attempt_id: "attempt-a".into(),
            descriptor: descriptor.clone(),
            previous: None,
            host_binding: Some("host-a".into()),
        };
        let staged = StagedArtifact {
            descriptor,
            staging_reference: "opaque-stage".into(),
        };
        let installed = root.join("observed-host.json");
        Self {
            root,
            journal,
            host: Host {
                calls: Mutex::new(Vec::new()),
                failure: Mutex::new(None),
                installed,
            },
            request,
            staged,
        }
    }
    fn flow(&self) -> HostUpdateFlow<'_> {
        HostUpdateFlow::new(&self.journal, &self.host, &self.host)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
#[test]
fn commit_failure_never_records_completion_and_restart_repeats_health() {
    let fixture = Fixture::new("commit");
    fixture
        .flow()
        .prepare(&fixture.request, &fixture.staged, 1)
        .unwrap();
    let evidence = fixture
        .flow()
        .activate(&fixture.request, &fixture.staged, 2)
        .unwrap();
    *fixture.host.failure.lock().unwrap() = Some("commit");
    assert!(fixture.flow().confirm(&evidence, 3).is_err());
    let reopened = FileRecoveryStore::open(&fixture.root).unwrap();
    assert!(
        matches!(reopened.inspect_recovery().unwrap(), RecoveryDecision::PendingActivation { receipt } if receipt.phase == ActivationPhaseV2::ActivatedPendingHealth)
    );
    *fixture.host.failure.lock().unwrap() = None;
    HostUpdateFlow::new(&reopened, &fixture.host, &fixture.host)
        .confirm(&evidence, 4)
        .unwrap();
    assert!(matches!(
        reopened.inspect_recovery().unwrap(),
        RecoveryDecision::Committed { .. }
    ));
    assert!(reopened
        .replay(RecoveryAction::ConfirmHealthy {
            attempt_id: "attempt-a".into(),
            activated_sha256: "22".repeat(32),
            at_ms: 5
        })
        .is_err());
}
#[test]
fn every_pre_health_failure_keeps_recoverable_intent_and_no_success() {
    for phase in ["snapshot", "prepare", "seal", "activate"] {
        let fixture = Fixture::new(phase);
        *fixture.host.failure.lock().unwrap() = Some(phase);
        let prepared = fixture.flow().prepare(&fixture.request, &fixture.staged, 1);
        if prepared.is_ok() {
            assert!(fixture
                .flow()
                .activate(&fixture.request, &fixture.staged, 2)
                .is_err());
        }
        assert!(
            matches!(fixture.journal.inspect_recovery().unwrap(), RecoveryDecision::PendingActivation { receipt } if receipt.phase == ActivationPhaseV2::Prepared)
        );
        assert!(!fixture.host.calls.lock().unwrap().contains(&"commit"));
    }
}
#[test]
fn health_failure_requires_real_rollback_evidence_and_rejects_forged_host() {
    let fixture = Fixture::new("health");
    fixture
        .flow()
        .prepare(&fixture.request, &fixture.staged, 1)
        .unwrap();
    let evidence = fixture
        .flow()
        .activate(&fixture.request, &fixture.staged, 2)
        .unwrap();
    let mut forged = evidence.clone();
    forged.host_binding = Some("other-host".into());
    assert!(fixture.flow().confirm(&forged, 3).is_err());
    *fixture.host.failure.lock().unwrap() = Some("health");
    assert!(fixture.flow().confirm(&evidence, 3).is_err());
    fixture.flow().rollback().unwrap();
    assert!(matches!(
        fixture.journal.inspect_recovery().unwrap(),
        RecoveryDecision::RollbackRequired { .. }
    ));
    fixture
        .journal
        .replay(RecoveryAction::RecordRolledBack {
            attempt_id: "attempt-a".into(),
            active_sha256: None,
            at_ms: 4,
        })
        .unwrap();
    assert!(fixture
        .journal
        .replay(RecoveryAction::RecordRolledBack {
            attempt_id: "attempt-a".into(),
            active_sha256: Some("22".repeat(32)),
            at_ms: 5
        })
        .is_err());
}
#[test]
fn restart_cannot_skip_failed_preparation_and_version_identity_is_not_reused() {
    let mut fixture = Fixture::new("restart");
    *fixture.host.failure.lock().unwrap() = Some("snapshot");
    assert!(fixture
        .flow()
        .prepare(&fixture.request, &fixture.staged, 1)
        .is_err());
    assert!(fixture
        .flow()
        .activate(&fixture.request, &fixture.staged, 2)
        .is_err());
    assert!(!fixture.host.calls.lock().unwrap().contains(&"activate"));
    fixture.journal.clear().unwrap();
    fixture.request.previous = Some(fixture.request.descriptor.clone());
    assert!(fixture
        .flow()
        .prepare(&fixture.request, &fixture.staged, 3)
        .is_err());
    assert_eq!(
        fixture.journal.inspect_recovery().unwrap(),
        RecoveryDecision::Idle
    );
}

#[test]
fn complete_flow_replay_is_idempotent_and_does_not_repeat_host_work() {
    let fixture = Fixture::new("replay");
    fixture
        .flow()
        .prepare(&fixture.request, &fixture.staged, 1)
        .unwrap();
    let evidence = fixture
        .flow()
        .activate(&fixture.request, &fixture.staged, 2)
        .unwrap();
    fixture.flow().confirm(&evidence, 3).unwrap();
    let calls = fixture.host.calls.lock().unwrap().clone();
    fixture
        .flow()
        .prepare(&fixture.request, &fixture.staged, 4)
        .unwrap();
    let replay = fixture
        .flow()
        .activate(&fixture.request, &fixture.staged, 4)
        .unwrap();
    assert_eq!(replay, evidence);
    fixture.flow().confirm(&replay, 4).unwrap();
    assert_eq!(*fixture.host.calls.lock().unwrap(), calls);
    assert!(!format!("{:?}", fixture.request).contains("host-a"));
    assert!(!format!("{evidence:?}").contains("host-a"));
}

fn predecessor(channel: &str, digest: &str) -> ArtifactDescriptor {
    ArtifactDescriptor::new(
        ApplicationId::new("app-a").unwrap(),
        "1.0.0",
        BuildId::new("build-a").unwrap(),
        channel,
        ">=1.0.0",
        "1",
        "memory:previous",
        digest,
        1,
    )
    .unwrap()
}

#[test]
fn automatic_host_upgrade_rejects_channel_switch_and_identical_package() {
    for (label, previous) in [
        ("channel", predecessor("preview", &"22".repeat(32))),
        ("same-package", predecessor("stable", &"11".repeat(32))),
    ] {
        let mut fixture = Fixture::new(label);
        fixture.request.previous = Some(previous);
        assert!(fixture
            .flow()
            .prepare(&fixture.request, &fixture.staged, 1)
            .is_err());
        assert_eq!(
            fixture.journal.inspect_recovery().unwrap(),
            RecoveryDecision::Idle
        );
        assert!(fixture.host.calls.lock().unwrap().is_empty());
    }
}

fn inspect_host(path: &std::path::Path) -> ActivationEvidence {
    let observation: ActivationReceiptV2 =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    ActivationEvidence {
        attempt_id: observation.attempt_id,
        descriptor: observation.activated,
        host_binding: observation.host_binding,
    }
}

#[test]
fn lost_activation_reply_is_reconciled_after_restart_without_reinstall_or_implicit_commit() {
    let fixture = Fixture::new("lost-activation");
    fixture
        .flow()
        .prepare(&fixture.request, &fixture.staged, 1)
        .unwrap();
    *fixture.host.failure.lock().unwrap() = Some("activate-reply");
    assert!(fixture
        .flow()
        .activate(&fixture.request, &fixture.staged, 2)
        .is_err());
    assert!(matches!(fixture.journal.inspect_recovery().unwrap(),
        RecoveryDecision::PendingActivation {receipt} if receipt.phase==ActivationPhaseV2::Prepared));
    let journal = FileRecoveryStore::open(&fixture.root).unwrap();
    let observed = inspect_host(&fixture.host.installed);
    let host = Host {
        calls: Mutex::new(Vec::new()),
        failure: Mutex::new(None),
        installed: fixture.host.installed.clone(),
    };
    let flow = HostUpdateFlow::new(&journal, &host, &host);
    flow.reconcile_activated(&observed, 3).unwrap();
    assert_eq!(*host.calls.lock().unwrap(), vec!["seal"]);
    assert!(matches!(journal.inspect_recovery().unwrap(),
        RecoveryDecision::PendingActivation {receipt} if receipt.phase==ActivationPhaseV2::ActivatedPendingHealth));
    flow.reconcile_activated(&observed, 3).unwrap();
    flow.confirm(&observed, 4).unwrap();
    assert_eq!(
        *host.calls.lock().unwrap(),
        vec!["seal", "seal", "health", "commit"]
    );
    let calls = host.calls.lock().unwrap().clone();
    flow.reconcile_activated(&observed, 5).unwrap();
    assert_eq!(*host.calls.lock().unwrap(), calls);
}

#[test]
fn reconciliation_rejects_changed_identity_time_and_unsealed_writes_without_advancing() {
    let fixture = Fixture::new("reconcile-rejected");
    fixture
        .flow()
        .prepare(&fixture.request, &fixture.staged, 10)
        .unwrap();
    let observed = ActivationEvidence {
        attempt_id: fixture.request.attempt_id.clone(),
        descriptor: fixture.request.descriptor.clone(),
        host_binding: fixture.request.host_binding.clone(),
    };
    let mut variants = vec![observed.clone(); 3];
    variants[0].attempt_id = "other".into();
    variants[1].host_binding = Some("other-instance-fence-snapshot".into());
    variants[2].descriptor = predecessor("stable", &"22".repeat(32));
    let calls = fixture.host.calls.lock().unwrap().clone();
    for wrong in variants {
        assert!(fixture.flow().reconcile_activated(&wrong, 11).is_err());
    }
    assert!(fixture.flow().reconcile_activated(&observed, 9).is_err());
    assert_eq!(*fixture.host.calls.lock().unwrap(), calls);
    *fixture.host.failure.lock().unwrap() = Some("seal");
    assert!(fixture.flow().reconcile_activated(&observed, 11).is_err());
    assert!(matches!(fixture.journal.inspect_recovery().unwrap(),
        RecoveryDecision::PendingActivation {receipt} if receipt.phase==ActivationPhaseV2::Prepared));
}

#[test]
fn reconciled_activation_still_requires_health_and_real_rollback_evidence() {
    let fixture = Fixture::new("reconciled-health");
    fixture
        .flow()
        .prepare(&fixture.request, &fixture.staged, 1)
        .unwrap();
    *fixture.host.failure.lock().unwrap() = Some("activate-reply");
    assert!(fixture
        .flow()
        .activate(&fixture.request, &fixture.staged, 2)
        .is_err());
    let observed = inspect_host(&fixture.host.installed);
    *fixture.host.failure.lock().unwrap() = None;
    fixture.flow().reconcile_activated(&observed, 3).unwrap();
    *fixture.host.failure.lock().unwrap() = Some("health");
    assert!(fixture.flow().confirm(&observed, 4).is_err());
    assert!(!fixture.host.calls.lock().unwrap().contains(&"commit"));
    fixture.flow().rollback().unwrap();
    assert!(matches!(
        fixture.journal.inspect_recovery().unwrap(),
        RecoveryDecision::RollbackRequired { .. }
    ));
    assert!(fixture.flow().reconcile_activated(&observed, 5).is_err());
}

#[test]
fn stored_prepared_intent_cannot_bypass_new_upgrade_admission() {
    for (label, previous) in [
        ("stored-channel", predecessor("preview", &"22".repeat(32))),
        ("stored-package", predecessor("stable", &"11".repeat(32))),
    ] {
        let mut fixture = Fixture::new(label);
        fixture.request.previous = Some(previous);
        fixture
            .journal
            .prepare(ActivationReceiptV2 {
                format_version: ACTIVATION_V2_FORMAT_VERSION,
                attempt_id: fixture.request.attempt_id.clone(),
                phase: ActivationPhaseV2::Prepared,
                activated: fixture.request.descriptor.clone(),
                previous: fixture.request.previous.clone(),
                created_at_ms: 1,
                updated_at_ms: 1,
                host_binding: fixture.request.host_binding.clone(),
                failure_reason: None,
            })
            .unwrap();
        let before = std::fs::read(fixture.root.join("activation-v2.json")).unwrap();
        assert!(fixture
            .flow()
            .activate(&fixture.request, &fixture.staged, 2)
            .is_err());
        assert!(fixture.host.calls.lock().unwrap().is_empty());
        assert_eq!(
            std::fs::read(fixture.root.join("activation-v2.json")).unwrap(),
            before
        );
    }
}
