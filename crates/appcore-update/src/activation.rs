// =============================================================================
//        #######
//     ###       ###     F: activation.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/25 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/25 00:00:00 by dnettoRaw
//      ###########      S: 1.0.3-rc
// =============================================================================

//! Generic host activation adapter contracts.

use crate::{
    ActivationReceiptV2, ArtifactDescriptor, RecoveryAction, RecoveryDecision, StagedArtifact,
    UpdateError, UpdateResult,
};

const MAX_ATTEMPT_ID_BYTES: usize = 128;
const MAX_HOST_BINDING_BYTES: usize = 512;

/// Runtime-owned context passed to a host activation adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationRequest {
    /// Fencing identity for this activation attempt.
    pub attempt_id: String,
    /// Verified artifact being prepared.
    pub descriptor: ArtifactDescriptor,
    /// Previously active artifact, when one exists.
    pub previous: Option<ArtifactDescriptor>,
    /// Opaque host-owned binding, when required by the adapter.
    pub host_binding: Option<String>,
}

impl ActivationRequest {
    /// Validates the bounded Runtime-to-host activation context.
    pub fn validate(&self) -> UpdateResult<()> {
        validate_text("attempt_id", &self.attempt_id, MAX_ATTEMPT_ID_BYTES)?;
        self.descriptor.validate()?;
        if let Some(previous) = &self.previous {
            previous.validate()?;
        }
        if let Some(binding) = &self.host_binding {
            validate_text("host_binding", binding, MAX_HOST_BINDING_BYTES)?;
        }
        Ok(())
    }
}

/// Host evidence returned after an adapter activates an artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationEvidence {
    /// Fencing identity repeated from the request.
    pub attempt_id: String,
    /// Artifact the host reports as active.
    pub descriptor: ArtifactDescriptor,
    /// Opaque host-owned binding observed during activation.
    pub host_binding: Option<String>,
}

impl ActivationEvidence {
    /// Validates bounded activation evidence before it enters a receipt.
    pub fn validate(&self) -> UpdateResult<()> {
        validate_text("attempt_id", &self.attempt_id, MAX_ATTEMPT_ID_BYTES)?;
        self.descriptor.validate()?;
        if let Some(binding) = &self.host_binding {
            validate_text("host_binding", binding, MAX_HOST_BINDING_BYTES)?;
        }
        Ok(())
    }

    /// Converts host evidence into the durable V2 receipt representation.
    pub fn into_receipt(
        self,
        previous: Option<ArtifactDescriptor>,
        created_at_ms: u64,
    ) -> UpdateResult<ActivationReceiptV2> {
        self.validate()?;
        if let Some(previous) = &previous {
            previous.validate()?;
        }
        Ok(ActivationReceiptV2 {
            format_version: crate::ACTIVATION_V2_FORMAT_VERSION,
            attempt_id: self.attempt_id,
            phase: crate::ActivationPhaseV2::ActivatedPendingHealth,
            activated: self.descriptor,
            previous,
            created_at_ms,
            updated_at_ms: created_at_ms,
            host_binding: self.host_binding,
            failure_reason: None,
        })
    }
}

/// Generic host adapter for prepare, activation, health, commit, rollback and recovery.
pub trait ActivationAdapter: Send + Sync {
    /// Prepares host-owned state without making the artifact active.
    fn prepare(&self, request: &ActivationRequest, staged: &StagedArtifact) -> UpdateResult<()>;
    /// Makes the staged artifact active and returns host evidence.
    fn activate(
        &self,
        request: &ActivationRequest,
        staged: &StagedArtifact,
    ) -> UpdateResult<ActivationEvidence>;
    /// Probes the active artifact through the host-owned health boundary.
    fn healthcheck(&self, evidence: &ActivationEvidence) -> UpdateResult<()>;
    /// Commits host-owned activation after Runtime health evidence is durable.
    fn commit(&self, receipt: &ActivationReceiptV2) -> UpdateResult<()>;
    /// Restores the previous artifact described by the receipt.
    fn rollback(&self, receipt: &ActivationReceiptV2) -> UpdateResult<()>;
    /// Converts persisted recovery evidence into one explicit host action.
    fn recover(&self, decision: &RecoveryDecision) -> UpdateResult<RecoveryAction>;
}

fn validate_text(name: &str, value: &str, max: usize) -> UpdateResult<()> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(UpdateError::Recovery(format!(
            "activation {name} is empty, too long or contains control characters"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use appcore_contracts::{ApplicationId, BuildId};

    fn descriptor() -> ArtifactDescriptor {
        ArtifactDescriptor::new(
            ApplicationId::new("app-a").unwrap(),
            "1.1.0",
            BuildId::new("build-a").unwrap(),
            "stable",
            ">=0.6.0, <1.0.0",
            "1",
            "memory:build-a",
            "00".repeat(32),
            1,
        )
        .unwrap()
    }

    #[test]
    fn activation_evidence_becomes_pending_v2_receipt() {
        let evidence = ActivationEvidence {
            attempt_id: "attempt-a".to_string(),
            descriptor: descriptor(),
            host_binding: Some("host-a".to_string()),
        };
        let receipt = evidence.into_receipt(None, 100).unwrap();
        assert_eq!(
            receipt.phase,
            crate::ActivationPhaseV2::ActivatedPendingHealth
        );
        assert_eq!(receipt.attempt_id, "attempt-a");
    }

    #[test]
    fn activation_context_rejects_unbounded_identifiers() {
        let request = ActivationRequest {
            attempt_id: "bad\nattempt".to_string(),
            descriptor: descriptor(),
            previous: None,
            host_binding: None,
        };
        assert!(matches!(
            request.validate(),
            Err(UpdateError::Recovery(message)) if message.contains("attempt_id")
        ));
    }
}
