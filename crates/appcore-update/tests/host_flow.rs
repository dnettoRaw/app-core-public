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
        Ok(ActivationEvidence {
            attempt_id: request.attempt_id.clone(),
            descriptor: request.descriptor.clone(),
            host_binding: request.host_binding.clone(),
        })
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
        Self {
            root,
            journal,
            host: Host {
                calls: Mutex::new(Vec::new()),
                failure: Mutex::new(None),
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
