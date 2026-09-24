// =============================================================================
//        #######
//     ###       ###     F: recovery.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/24 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/24 00:00:00 by dnettoRaw
//      ###########      S: 1.0.3-rc
// =============================================================================

//! Versioned activation receipts and explicit recovery replay.

use crate::store_io::{
    atomic_write_json, read_json_bounded, remove_if_exists, JsonReadError,
    MAX_UPDATE_METADATA_BYTES,
};
use crate::{ArtifactDescriptor, UpdateError, UpdateResult};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Version of the additive activation recovery journal.
pub const ACTIVATION_V2_FORMAT_VERSION: u16 = 2;
const MAX_ATTEMPT_ID_BYTES: usize = 128;
const MAX_REASON_BYTES: usize = 512;
const MAX_HOST_BINDING_BYTES: usize = 512;

/// Persisted phase of one V2 activation attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationPhaseV2 {
    /// Artifact is prepared but not active.
    Prepared,
    /// Artifact is active and awaits host health evidence.
    ActivatedPendingHealth,
    /// Host confirmed the activated artifact.
    Committed,
    /// Activation failed and the host must explicitly restore the previous artifact.
    RollbackRequired,
    /// The host confirmed that the previous artifact was restored.
    RolledBack,
    /// A preparation was explicitly discarded before activation.
    Aborted,
    /// Evidence conflicts or is insufficient for an automatic action.
    ManualReviewRequired,
}

/// Versioned V2 activation receipt kept separately from the V1 receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivationReceiptV2 {
    /// Persisted receipt format.
    pub format_version: u16,
    /// Unique attempt identity used for fencing and replay.
    pub attempt_id: String,
    /// Current activation phase.
    pub phase: ActivationPhaseV2,
    /// Artifact being activated.
    pub activated: ArtifactDescriptor,
    /// Artifact that the host may restore, when one exists.
    pub previous: Option<ArtifactDescriptor>,
    /// Creation timestamp supplied by the deployment clock.
    pub created_at_ms: u64,
    /// Last phase transition timestamp supplied by the deployment clock.
    pub updated_at_ms: u64,
    /// Optional opaque host binding, never interpreted by the Runtime.
    pub host_binding: Option<String>,
    /// Bounded failure detail for the recovery operator.
    pub failure_reason: Option<String>,
}

impl ActivationReceiptV2 {
    /// Validates the receipt without consulting external installation state.
    pub fn validate(&self) -> UpdateResult<()> {
        if self.format_version != ACTIVATION_V2_FORMAT_VERSION {
            return Err(UpdateError::Recovery(
                "NO MORE SUPPORTED PLEASE UPDATE".to_string(),
            ));
        }
        validate_text("attempt_id", &self.attempt_id, MAX_ATTEMPT_ID_BYTES)?;
        if self.updated_at_ms < self.created_at_ms {
            return Err(UpdateError::Recovery(
                "receipt transition timestamp precedes creation".to_string(),
            ));
        }
        self.activated.validate()?;
        if let Some(previous) = &self.previous {
            previous.validate()?;
        }
        if let Some(binding) = &self.host_binding {
            validate_text("host_binding", binding, MAX_HOST_BINDING_BYTES)?;
        }
        if let Some(reason) = &self.failure_reason {
            validate_text("failure_reason", reason, MAX_REASON_BYTES)?;
        }
        Ok(())
    }
}

/// Read-only conclusion from one recovery inspection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryDecision {
    /// No V2 receipt is present.
    Idle,
    /// A receipt requires an explicit host action.
    PendingActivation {
        /// Pending receipt.
        receipt: ActivationReceiptV2,
    },
    /// The attempt is durably committed.
    Committed {
        /// Committed receipt.
        receipt: ActivationReceiptV2,
    },
    /// The host must restore the previous artifact.
    RollbackRequired {
        /// Receipt requiring host rollback.
        receipt: ActivationReceiptV2,
    },
    /// The previous artifact was restored and recorded.
    RolledBack {
        /// Receipt with externally confirmed rollback.
        receipt: ActivationReceiptV2,
    },
    /// A preparation was explicitly discarded before activation.
    Aborted {
        /// Receipt explicitly discarded before activation.
        receipt: ActivationReceiptV2,
    },
    /// Automatic replay is forbidden until an operator reviews the evidence.
    ManualReviewRequired {
        /// Bounded reason for manual review.
        reason: String,
    },
}

/// Explicit host action applied to a V2 recovery receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryAction {
    /// Confirms health and commits the currently active artifact.
    ConfirmHealthy {
        /// Attempt being confirmed.
        attempt_id: String,
        /// Digest observed by the host as active.
        activated_sha256: String,
        /// Timestamp of the host evidence.
        at_ms: u64,
    },
    /// Records that health failed and makes rollback required.
    RequireRollback {
        /// Attempt whose activation failed.
        attempt_id: String,
        /// Bounded operator-visible reason.
        reason: String,
        /// Timestamp of the host evidence.
        at_ms: u64,
    },
    /// Records an externally completed rollback.
    RecordRolledBack {
        /// Attempt being finalized.
        attempt_id: String,
        /// Digest observed as active after rollback, if one exists.
        active_sha256: Option<String>,
        /// Timestamp of the host evidence.
        at_ms: u64,
    },
    /// Discards a preparation that was never activated.
    DiscardPrepared {
        /// Attempt being discarded.
        attempt_id: String,
        /// Timestamp of the discard decision.
        at_ms: u64,
    },
    /// Moves the receipt to manual review without guessing an outcome.
    MarkManualReview {
        /// Attempt being quarantined for review.
        attempt_id: String,
        /// Bounded operator-visible reason.
        reason: String,
        /// Timestamp of the decision.
        at_ms: u64,
    },
}

/// Result of an explicit recovery action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryResult {
    /// Phase after the action.
    pub phase: ActivationPhaseV2,
    /// Whether the action observed an already-completed identical transition.
    pub idempotent: bool,
}

/// Filesystem journal for the additive V2 recovery contract.
#[derive(Debug, Clone)]
pub struct FileRecoveryStore {
    root: PathBuf,
}

impl FileRecoveryStore {
    /// Opens or creates a V2 recovery journal root.
    pub fn open(root: impl Into<PathBuf>) -> UpdateResult<Self> {
        let store = Self { root: root.into() };
        fs::create_dir_all(&store.root).map_err(store_error)?;
        reject_directory(&store.root)?;
        Ok(store)
    }

    /// Writes a new prepared receipt and rejects an existing attempt.
    pub fn prepare(&self, receipt: ActivationReceiptV2) -> UpdateResult<()> {
        receipt.validate()?;
        if receipt.phase != ActivationPhaseV2::Prepared {
            return Err(UpdateError::Recovery(
                "new V2 receipts must start in prepared phase".to_string(),
            ));
        }
        if self.read_receipt()?.is_some() {
            return Err(UpdateError::Recovery(
                "another V2 activation attempt is already pending".to_string(),
            ));
        }
        atomic_write_json(&self.receipt_path(), &receipt)
    }

    /// Marks a prepared receipt as active and awaiting host health evidence.
    pub fn mark_activated(&self, attempt_id: &str, at_ms: u64) -> UpdateResult<RecoveryResult> {
        let receipt = self
            .read_receipt()?
            .ok_or_else(|| UpdateError::Recovery("recovery journal is idle".to_string()))?;
        if receipt.attempt_id != attempt_id {
            return Err(UpdateError::Recovery(
                "recovery action belongs to another attempt".to_string(),
            ));
        }
        if receipt.phase == ActivationPhaseV2::ActivatedPendingHealth {
            return Ok(RecoveryResult {
                phase: receipt.phase,
                idempotent: true,
            });
        }
        if receipt.phase != ActivationPhaseV2::Prepared {
            return Err(UpdateError::Recovery(
                "activation evidence conflicts with the persisted phase".to_string(),
            ));
        }
        self.write_phase(receipt, ActivationPhaseV2::ActivatedPendingHealth, at_ms)
    }

    /// Inspects the journal without changing or removing any file.
    pub fn inspect_recovery(&self) -> UpdateResult<RecoveryDecision> {
        let Some(receipt) = self.read_receipt()? else {
            return Ok(RecoveryDecision::Idle);
        };
        match receipt.phase {
            ActivationPhaseV2::Prepared | ActivationPhaseV2::ActivatedPendingHealth => {
                Ok(RecoveryDecision::PendingActivation { receipt })
            }
            ActivationPhaseV2::Committed => Ok(RecoveryDecision::Committed { receipt }),
            ActivationPhaseV2::RollbackRequired => {
                Ok(RecoveryDecision::RollbackRequired { receipt })
            }
            ActivationPhaseV2::RolledBack => Ok(RecoveryDecision::RolledBack { receipt }),
            ActivationPhaseV2::Aborted => Ok(RecoveryDecision::Aborted { receipt }),
            ActivationPhaseV2::ManualReviewRequired => Ok(RecoveryDecision::ManualReviewRequired {
                reason: receipt
                    .failure_reason
                    .unwrap_or_else(|| "manual review required".to_string()),
            }),
        }
    }

    /// Applies one explicitly authorized and fenced recovery action.
    pub fn replay(&self, action: RecoveryAction) -> UpdateResult<RecoveryResult> {
        let Some(receipt) = self.read_receipt()? else {
            return Err(UpdateError::Recovery(
                "recovery journal is idle".to_string(),
            ));
        };
        let (attempt_id, at_ms) = action_identity(&action);
        if attempt_id != receipt.attempt_id {
            return Err(UpdateError::Recovery(
                "recovery action belongs to another attempt".to_string(),
            ));
        }
        let previous = receipt.phase;
        let receipt = apply_action(receipt, action)?;
        let phase = receipt.phase;
        let idempotent = previous == phase;
        self.write_receipt(receipt, at_ms)?;
        Ok(RecoveryResult { phase, idempotent })
    }

    /// Removes a V2 journal only when the caller explicitly discards it.
    pub fn clear(&self) -> UpdateResult<()> {
        remove_if_exists(&self.receipt_path())
    }

    fn write_phase(
        &self,
        mut receipt: ActivationReceiptV2,
        phase: ActivationPhaseV2,
        at_ms: u64,
    ) -> UpdateResult<RecoveryResult> {
        let previous = receipt.phase;
        receipt.phase = phase;
        self.write_receipt(receipt, at_ms)?;
        Ok(RecoveryResult {
            phase,
            idempotent: previous == phase,
        })
    }

    fn write_receipt(&self, mut receipt: ActivationReceiptV2, at_ms: u64) -> UpdateResult<()> {
        receipt.updated_at_ms = at_ms;
        receipt.validate()?;
        atomic_write_json(&self.receipt_path(), &receipt)
    }

    fn read_receipt(&self) -> UpdateResult<Option<ActivationReceiptV2>> {
        match read_json_bounded::<ActivationReceiptV2>(
            &self.receipt_path(),
            MAX_UPDATE_METADATA_BYTES,
        ) {
            Ok(receipt) => {
                if let Some(receipt) = &receipt {
                    receipt.validate()?;
                }
                Ok(receipt)
            }
            Err(JsonReadError::Io(error)) => Err(store_error(error)),
            Err(JsonReadError::Decode(_)) => Err(UpdateError::Recovery(
                "NO MORE SUPPORTED PLEASE UPDATE".to_string(),
            )),
        }
    }

    fn receipt_path(&self) -> PathBuf {
        self.root.join("activation-v2.json")
    }
}

fn apply_action(
    mut receipt: ActivationReceiptV2,
    action: RecoveryAction,
) -> UpdateResult<ActivationReceiptV2> {
    match action {
        RecoveryAction::ConfirmHealthy {
            activated_sha256, ..
        } => {
            if receipt.phase == ActivationPhaseV2::Committed {
                return Ok(receipt);
            }
            if receipt.phase != ActivationPhaseV2::ActivatedPendingHealth
                || receipt.activated.sha256() != activated_sha256
            {
                return conflict();
            }
            receipt.phase = ActivationPhaseV2::Committed;
            receipt.failure_reason = None;
        }
        RecoveryAction::RequireRollback { reason, .. } => {
            validate_text("failure_reason", &reason, MAX_REASON_BYTES)?;
            if receipt.phase == ActivationPhaseV2::RollbackRequired {
                return Ok(receipt);
            }
            if receipt.phase != ActivationPhaseV2::ActivatedPendingHealth {
                return conflict();
            }
            receipt.phase = ActivationPhaseV2::RollbackRequired;
            receipt.failure_reason = Some(reason);
        }
        RecoveryAction::RecordRolledBack { active_sha256, .. } => {
            if receipt.phase == ActivationPhaseV2::RolledBack {
                return Ok(receipt);
            }
            if receipt.phase != ActivationPhaseV2::RollbackRequired {
                return conflict();
            }
            match (&receipt.previous, active_sha256) {
                (None, None) => {}
                (Some(previous), Some(active)) if previous.sha256() == active => {}
                _ => return conflict(),
            }
            receipt.phase = ActivationPhaseV2::RolledBack;
        }
        RecoveryAction::DiscardPrepared { .. } => {
            if receipt.phase != ActivationPhaseV2::Prepared {
                return conflict();
            }
            receipt.phase = ActivationPhaseV2::Aborted;
        }
        RecoveryAction::MarkManualReview { reason, .. } => {
            validate_text("failure_reason", &reason, MAX_REASON_BYTES)?;
            if receipt.phase == ActivationPhaseV2::ManualReviewRequired {
                return Ok(receipt);
            }
            receipt.phase = ActivationPhaseV2::ManualReviewRequired;
            receipt.failure_reason = Some(reason);
        }
    }
    Ok(receipt)
}

fn action_identity(action: &RecoveryAction) -> (&str, u64) {
    match action {
        RecoveryAction::ConfirmHealthy {
            attempt_id, at_ms, ..
        }
        | RecoveryAction::RequireRollback {
            attempt_id, at_ms, ..
        }
        | RecoveryAction::RecordRolledBack {
            attempt_id, at_ms, ..
        }
        | RecoveryAction::DiscardPrepared { attempt_id, at_ms }
        | RecoveryAction::MarkManualReview {
            attempt_id, at_ms, ..
        } => (attempt_id, *at_ms),
    }
}

fn validate_text(name: &str, value: &str, max_bytes: usize) -> UpdateResult<()> {
    if value.trim().is_empty() || value.len() > max_bytes || value.chars().any(char::is_control) {
        return Err(UpdateError::Recovery(format!("{name} is invalid")));
    }
    Ok(())
}

fn conflict<T>() -> UpdateResult<T> {
    Err(UpdateError::Recovery(
        "recovery action conflicts with the persisted phase or evidence".to_string(),
    ))
}

fn reject_directory(path: &Path) -> UpdateResult<()> {
    let metadata = fs::symlink_metadata(path).map_err(store_error)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(UpdateError::Store(
            "recovery root is not a regular directory".to_string(),
        ));
    }
    Ok(())
}

fn store_error(error: std::io::Error) -> UpdateError {
    UpdateError::Store(error.to_string())
}
