// =============================================================================
//        #######
//     ###       ###     F: host_flow.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/12 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/12 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Explicit host composition over existing V2 receipts, without owning a process.
use crate::{
    ActivationAdapter, ActivationEvidence, ActivationPhaseV2, ActivationReceiptV2,
    ActivationRequest, FileRecoveryStore, RecoveryAction, RecoveryDecision, StagedArtifact,
    UpdateError, UpdateResult,
};

/// Host-owned write exclusion and data recovery preparation.
/// Implementations must impose deadlines, persist installation-bound backups,
/// and keep normal writes blocked on every ambiguous result, including restart.
pub trait UpdateRecoveryGate: Send + Sync {
    /// Idempotently blocks/drains writes and verifies a recoverable data snapshot.
    fn drain_and_snapshot(&self, request: &ActivationRequest) -> UpdateResult<()>;
    /// Seals normal write admission before host installation may begin.
    fn seal(&self, request: &ActivationRequest) -> UpdateResult<()>;
}

/// One deployment-owned adapter composition. Use a single external process lock
/// for the journal and host operation. This type never acquires a hidden lock,
/// starts a process, selects providers or performs a domain migration.
pub struct HostUpdateFlow<'a> {
    journal: &'a FileRecoveryStore,
    adapter: &'a dyn ActivationAdapter,
    gate: &'a dyn UpdateRecoveryGate,
}
impl<'a> HostUpdateFlow<'a> {
    /// Composes an explicitly locked journal, host adapter and recovery gate.
    pub fn new(
        journal: &'a FileRecoveryStore,
        adapter: &'a dyn ActivationAdapter,
        gate: &'a dyn UpdateRecoveryGate,
    ) -> Self {
        Self {
            journal,
            adapter,
            gate,
        }
    }
    /// Persists intent before drain, snapshot and adapter preparation. On any
    /// failure the prepared receipt remains inspectable; recovery must inspect
    /// host reality rather than assuming that an interrupted callback did nothing.
    /// Supply only bytes already authenticated/verified/staged by Update.
    pub fn prepare(
        &self,
        request: &ActivationRequest,
        staged: &StagedArtifact,
        at_ms: u64,
    ) -> UpdateResult<()> {
        request.validate()?;
        if staged.descriptor != request.descriptor {
            return conflict();
        }
        validate_upgrade(request)?;
        match self.journal.inspect_recovery()? {
            RecoveryDecision::Idle => self.journal.prepare(ActivationReceiptV2 {
                format_version: crate::ACTIVATION_V2_FORMAT_VERSION,
                attempt_id: request.attempt_id.clone(),
                phase: ActivationPhaseV2::Prepared,
                activated: request.descriptor.clone(),
                previous: request.previous.clone(),
                created_at_ms: at_ms,
                updated_at_ms: at_ms,
                host_binding: request.host_binding.clone(),
                failure_reason: None,
            })?,
            RecoveryDecision::PendingActivation { receipt }
            | RecoveryDecision::Committed { receipt } => {
                check_request(&receipt, request)?;
                if receipt.phase != ActivationPhaseV2::Prepared {
                    return Ok(());
                }
            }
            _ => return conflict(),
        }
        self.gate.drain_and_snapshot(request)?;
        self.adapter.prepare(request, staged)
    }
    /// Repeats the idempotent preparation boundary before activation so a
    /// restarted caller cannot skip a previously failed drain/snapshot callback.
    /// A failed activation leaves prepared intent; inspect host reality on restart.
    pub fn activate(
        &self,
        request: &ActivationRequest,
        staged: &StagedArtifact,
        at_ms: u64,
    ) -> UpdateResult<ActivationEvidence> {
        let receipt = self.activation_receipt()?;
        if staged.descriptor != request.descriptor {
            return conflict();
        }
        check_request(&receipt, request)?;
        if matches!(
            receipt.phase,
            ActivationPhaseV2::ActivatedPendingHealth | ActivationPhaseV2::Committed
        ) {
            return Ok(ActivationEvidence {
                attempt_id: receipt.attempt_id,
                descriptor: receipt.activated,
                host_binding: receipt.host_binding,
            });
        }
        check_timestamp(&receipt, at_ms)?;
        validate_upgrade(request)?;
        self.gate.drain_and_snapshot(request)?;
        self.adapter.prepare(request, staged)?;
        self.gate.seal(request)?;
        let evidence = self.adapter.activate(request, staged)?;
        check_evidence(&receipt, &evidence)?;
        self.journal.mark_activated(&receipt.attempt_id, at_ms)?;
        Ok(evidence)
    }
    /// Records an exact candidate observed as active after an interrupted activation.
    /// The trusted host must inspect the real installed/running artifact, attempt and
    /// installation/snapshot/instance binding under its original exclusive lock/fence.
    /// Reverify installed bytes, authenticity, deployment policy and the bound recovery snapshot.
    /// Do not derive this evidence merely from Prepared intent or an installer exit code.
    /// Re-seals writes, then moves Prepared to ActivatedPendingHealth without invoking
    /// prepare/activate, healthcheck or commit. Health confirmation remains a separate step.
    /// Ambiguous inspection must stay blocked for manual review. The timestamp must not
    /// precede the last persisted transition; idempotent observations do not advance it.
    pub fn reconcile_activated(
        &self,
        observed: &ActivationEvidence,
        at_ms: u64,
    ) -> UpdateResult<()> {
        let receipt = self.activation_receipt()?;
        check_evidence(&receipt, observed)?;
        check_timestamp(&receipt, at_ms)?;
        if receipt.phase == ActivationPhaseV2::Committed {
            return Ok(());
        }
        let request = ActivationRequest {
            attempt_id: receipt.attempt_id.clone(),
            descriptor: receipt.activated.clone(),
            previous: receipt.previous.clone(),
            host_binding: receipt.host_binding.clone(),
        };
        self.gate.seal(&request)?;
        self.journal.mark_activated(&receipt.attempt_id, at_ms)?;
        Ok(())
    }

    /// Confirms host health, commits the adapter, then records completion.
    /// Backend and renderer readiness must both be included in a desktop adapter.
    /// Failed health durably requests rollback. Failed/ambiguous commit retains
    /// pending intent and never reports success or guesses a rollback.
    pub fn confirm(&self, evidence: &ActivationEvidence, at_ms: u64) -> UpdateResult<()> {
        let receipt = self.activation_receipt()?;
        check_evidence(&receipt, evidence)?;
        if receipt.phase == ActivationPhaseV2::Committed {
            return Ok(());
        }
        if receipt.phase != ActivationPhaseV2::ActivatedPendingHealth {
            return conflict();
        }
        check_timestamp(&receipt, at_ms)?;
        if self.adapter.healthcheck(evidence).is_err() {
            self.journal.replay(RecoveryAction::RequireRollback {
                attempt_id: receipt.attempt_id,
                reason: "host health gate failed".into(),
                at_ms,
            })?;
            return Err(UpdateError::Health("host health gate failed".into()));
        }
        self.adapter.commit(&receipt)?;
        self.journal.replay(RecoveryAction::ConfirmHealthy {
            attempt_id: receipt.attempt_id,
            activated_sha256: evidence.descriptor.sha256().into(),
            at_ms,
        })?;
        Ok(())
    }
    /// Runs host rollback but does not record completion. The host must re-probe
    /// the actual restored identity and supply `RecordRolledBack` to the journal.
    pub fn rollback(&self) -> UpdateResult<()> {
        match self.journal.inspect_recovery()? {
            RecoveryDecision::RollbackRequired { receipt } => self.adapter.rollback(&receipt),
            RecoveryDecision::RolledBack { .. } => Ok(()),
            _ => conflict(),
        }
    }
    fn activation_receipt(&self) -> UpdateResult<ActivationReceiptV2> {
        match self.journal.inspect_recovery()? {
            RecoveryDecision::PendingActivation { receipt }
            | RecoveryDecision::Committed { receipt } => Ok(receipt),
            _ => conflict(),
        }
    }
}
fn check_request(receipt: &ActivationReceiptV2, request: &ActivationRequest) -> UpdateResult<()> {
    request.validate()?;
    if receipt.attempt_id != request.attempt_id
        || receipt.activated != request.descriptor
        || receipt.previous != request.previous
        || receipt.host_binding != request.host_binding
    {
        return conflict();
    }
    Ok(())
}
fn check_evidence(
    receipt: &ActivationReceiptV2,
    evidence: &ActivationEvidence,
) -> UpdateResult<()> {
    evidence.validate()?;
    if receipt.attempt_id != evidence.attempt_id
        || receipt.activated != evidence.descriptor
        || receipt.host_binding != evidence.host_binding
    {
        return conflict();
    }
    Ok(())
}
fn validate_upgrade(request: &ActivationRequest) -> UpdateResult<()> {
    if let Some(previous) = &request.previous {
        if previous.application_id() != request.descriptor.application_id()
            || previous.channel() != request.descriptor.channel()
            || previous.sha256() == request.descriptor.sha256()
            || previous.build_id() == request.descriptor.build_id()
            || previous.target() != request.descriptor.target()
            || semver::Version::parse(previous.application_version())
                .map_err(|_| UpdateError::Incompatible("invalid previous version".into()))?
                >= semver::Version::parse(request.descriptor.application_version())
                    .map_err(|_| UpdateError::Incompatible("invalid candidate version".into()))?
        {
            return Err(UpdateError::Incompatible(
                "candidate must advance the same application, channel and target with distinct bytes".into(),
            ));
        }
    }
    Ok(())
}
fn conflict<T>() -> UpdateResult<T> {
    Err(UpdateError::Recovery(
        "host operation conflicts with activation evidence".into(),
    ))
}

fn check_timestamp(receipt: &ActivationReceiptV2, at_ms: u64) -> UpdateResult<()> {
    if at_ms < receipt.updated_at_ms {
        return conflict();
    }
    Ok(())
}
