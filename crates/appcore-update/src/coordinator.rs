// =============================================================================
//        #######
//     ###       ###     F: coordinator.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 11:51:10 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use crate::{
    sha256_hex, ActivationReceipt, ArtifactAuthenticityVerifier, ArtifactDescriptor, ArtifactStore,
    StagedArtifact, UpdateError, UpdateProvider, UpdateRequest, UpdateResult,
};
use semver::Version;

#[cfg(test)]
struct TestOnlyArtifactVerifier;

#[cfg(test)]
impl ArtifactAuthenticityVerifier for TestOnlyArtifactVerifier {
    fn verify(&self, _artifact: &ArtifactDescriptor) -> UpdateResult<()> {
        Ok(())
    }
}

#[cfg(test)]
// appcore-norm: allow(global-state) reason: feature-gated verifier is immutable and stateless
static TEST_ONLY_ARTIFACTS: TestOnlyArtifactVerifier = TestOnlyArtifactVerifier;

struct VerifiedArtifact {
    descriptor: ArtifactDescriptor,
    bytes: Vec<u8>,
}

struct ActivatedArtifact {
    descriptor: ArtifactDescriptor,
    receipt: ActivationReceipt,
}

/// Health gate executed after an artifact becomes active.
pub trait ActivationHealthCheck: Send + Sync {
    /// Returns success only when the activated application is healthy.
    fn check(&self, artifact: &ArtifactDescriptor) -> UpdateResult<()>;
}

/// Controlled lifecycle points available to fault-injection tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateFaultPoint {
    /// After candidate selection and compatibility validation.
    AfterSelection,
    /// After download and checksum verification.
    AfterVerification,
    /// After staging but before activation.
    AfterStaging,
    /// After activation but before health verification.
    AfterActivation,
    /// After health verification but before commit.
    BeforeCommit,
}

/// Fault-injection contract used by deterministic update tests.
pub trait UpdateFaultInjector: Send + Sync {
    /// Returns an error when execution should fail at `point`.
    fn check(&self, point: UpdateFaultPoint) -> UpdateResult<()>;
}

/// Fault injector that never interrupts production execution.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoUpdateFaults;

impl UpdateFaultInjector for NoUpdateFaults {
    fn check(&self, _point: UpdateFaultPoint) -> UpdateResult<()> {
        Ok(())
    }
}

/// Final result of one update attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateOutcome {
    /// Provider reported no eligible update.
    NoUpdate,
    /// Artifact passed activation and health verification.
    Applied(ArtifactDescriptor),
    /// Activation failed and the previous artifact was restored.
    RolledBack {
        /// Artifact whose activation failed.
        attempted: ArtifactDescriptor,
        /// Controlled failure that triggered rollback.
        reason: String,
    },
}

/// Result of staging and activating an update before process-level health verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdatePreparation {
    /// Provider reported no eligible update.
    NoUpdate,
    /// Candidate is active and awaits supervisor health verification.
    AwaitingHealth(Box<ArtifactDescriptor>),
}

/// Result of candidate verification and staging before activation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateStaging {
    /// Provider reported no eligible update.
    NoUpdate,
    /// Verified candidate is staged but not active.
    Staged(Box<StagedArtifact>),
}

/// Coordinates provider, store, integrity, health and rollback boundaries.
pub struct UpdateCoordinator<'a> {
    provider: &'a dyn UpdateProvider,
    store: &'a dyn ArtifactStore,
    health: Option<&'a dyn ActivationHealthCheck>,
    authenticity: &'a dyn ArtifactAuthenticityVerifier,
    max_artifact_bytes: usize,
}

impl<'a> UpdateCoordinator<'a> {
    /// Creates a coordinator with an explicit artifact byte bound.
    pub fn new(
        provider: &'a dyn UpdateProvider,
        store: &'a dyn ArtifactStore,
        health: &'a dyn ActivationHealthCheck,
        max_artifact_bytes: usize,
    ) -> UpdateResult<Self> {
        #[cfg(test)]
        {
            Self::new_with_authenticity(
                provider,
                store,
                health,
                &TEST_ONLY_ARTIFACTS,
                max_artifact_bytes,
            )
        }
        #[cfg(not(test))]
        {
            let _ = (provider, store, health, max_artifact_bytes);
            Err(UpdateError::Authenticity(
                "an explicit artifact authenticity verifier is required".to_string(),
            ))
        }
    }

    /// Creates a coordinator with an explicit artifact authenticity policy.
    pub fn new_with_authenticity(
        provider: &'a dyn UpdateProvider,
        store: &'a dyn ArtifactStore,
        health: &'a dyn ActivationHealthCheck,
        authenticity: &'a dyn ArtifactAuthenticityVerifier,
        max_artifact_bytes: usize,
    ) -> UpdateResult<Self> {
        if max_artifact_bytes == 0 {
            return Err(UpdateError::InvalidArtifact(
                "max_artifact_bytes must be greater than zero".to_string(),
            ));
        }
        Ok(Self {
            provider,
            store,
            health: Some(health),
            authenticity,
            max_artifact_bytes,
        })
    }

    /// Creates a staging coordinator for process-level health verification.
    pub fn new_for_preparation(
        provider: &'a dyn UpdateProvider,
        store: &'a dyn ArtifactStore,
        authenticity: &'a dyn ArtifactAuthenticityVerifier,
        max_artifact_bytes: usize,
    ) -> UpdateResult<Self> {
        if max_artifact_bytes == 0 {
            return Err(UpdateError::InvalidArtifact(
                "max_artifact_bytes must be greater than zero".to_string(),
            ));
        }
        Ok(Self {
            provider,
            store,
            health: None,
            authenticity,
            max_artifact_bytes,
        })
    }

    /// Applies an update using the production no-fault path.
    pub fn apply(
        &self,
        request: &UpdateRequest,
        runtime_version: &str,
        protocol_version: &str,
    ) -> UpdateResult<UpdateOutcome> {
        self.apply_with_faults(request, runtime_version, protocol_version, &NoUpdateFaults)
    }

    /// Stages and activates an update without committing it.
    ///
    /// An application parent uses this two-phase path when the candidate must be
    /// restarted and probed before [`ArtifactStore::commit`] or rollback.
    pub fn prepare(
        &self,
        request: &UpdateRequest,
        runtime_version: &str,
        protocol_version: &str,
    ) -> UpdateResult<UpdatePreparation> {
        match self.stage_candidate(request, runtime_version, protocol_version)? {
            UpdateStaging::NoUpdate => Ok(UpdatePreparation::NoUpdate),
            UpdateStaging::Staged(staged) => {
                let receipt = self.store.activate(*staged)?;
                Ok(UpdatePreparation::AwaitingHealth(Box::new(
                    receipt.activated,
                )))
            }
        }
    }

    /// Verifies and stages a candidate without changing the active artifact.
    pub fn stage_candidate(
        &self,
        request: &UpdateRequest,
        runtime_version: &str,
        protocol_version: &str,
    ) -> UpdateResult<UpdateStaging> {
        self.store.recover()?;
        let Some(candidate) = self.select_candidate(request, runtime_version, protocol_version)?
        else {
            return Ok(UpdateStaging::NoUpdate);
        };
        let verified = self.fetch_verified(candidate)?;
        let staged = self.store.stage(&verified.descriptor, &verified.bytes)?;
        Ok(UpdateStaging::Staged(Box::new(staged)))
    }

    /// Applies an update while exposing deterministic lifecycle fault points.
    pub fn apply_with_faults(
        &self,
        request: &UpdateRequest,
        runtime_version: &str,
        protocol_version: &str,
        faults: &dyn UpdateFaultInjector,
    ) -> UpdateResult<UpdateOutcome> {
        self.store.recover()?;
        let Some(candidate) = self.select_candidate(request, runtime_version, protocol_version)?
        else {
            return Ok(UpdateOutcome::NoUpdate);
        };
        faults.check(UpdateFaultPoint::AfterSelection)?;
        let verified = self.fetch_verified(candidate)?;
        faults.check(UpdateFaultPoint::AfterVerification)?;
        let activated = self.stage_and_activate(verified, faults)?;
        self.finish_activation(activated, faults)
    }

    fn select_candidate(
        &self,
        request: &UpdateRequest,
        runtime_version: &str,
        protocol_version: &str,
    ) -> UpdateResult<Option<ArtifactDescriptor>> {
        let Some(candidate) = self.provider.latest(request)? else {
            return Ok(None);
        };
        if candidate.application_id() != &request.application_id {
            return Err(UpdateError::Incompatible(
                "artifact application identity differs from the request".to_string(),
            ));
        }
        if candidate.channel() != request.channel {
            return Err(UpdateError::Incompatible(
                "artifact update channel differs from the request".to_string(),
            ));
        }
        self.ensure_upgrade(request, &candidate)?;
        candidate.ensure_compatible(runtime_version, protocol_version)?;
        Ok(Some(candidate))
    }

    fn ensure_upgrade(
        &self,
        request: &UpdateRequest,
        candidate: &ArtifactDescriptor,
    ) -> UpdateResult<()> {
        let installed = Version::parse(&request.current_version).map_err(|error| {
            UpdateError::Incompatible(format!("invalid installed application version: {error}"))
        })?;
        let candidate_version =
            Version::parse(candidate.application_version()).map_err(|error| {
                UpdateError::InvalidArtifact(format!(
                    "invalid candidate application version: {error}"
                ))
            })?;
        if candidate_version <= installed {
            return Err(UpdateError::Incompatible(format!(
                "candidate version {candidate_version} does not advance installed version {installed}"
            )));
        }
        let Some(active) = self.store.current()? else {
            return Ok(());
        };
        if active.application_id() != candidate.application_id() {
            return Err(UpdateError::Incompatible(
                "active artifact application identity differs from the candidate".to_string(),
            ));
        }
        if active.build_id() == candidate.build_id() {
            return Err(UpdateError::Incompatible(
                "candidate reuses the active build identity".to_string(),
            ));
        }
        let active_version = Version::parse(active.application_version()).map_err(|error| {
            UpdateError::Store(format!("active artifact version is invalid: {error}"))
        })?;
        if candidate_version <= active_version {
            return Err(UpdateError::Incompatible(format!(
                "candidate version {candidate_version} does not advance active version {active_version}"
            )));
        }
        Ok(())
    }

    fn fetch_verified(&self, candidate: ArtifactDescriptor) -> UpdateResult<VerifiedArtifact> {
        let declared_size =
            usize::try_from(candidate.size_bytes()).map_err(|_| UpdateError::ArtifactTooLarge {
                max_bytes: self.max_artifact_bytes,
            })?;
        if declared_size > self.max_artifact_bytes {
            return Err(UpdateError::ArtifactTooLarge {
                max_bytes: self.max_artifact_bytes,
            });
        }
        let bytes = self.provider.fetch(&candidate, self.max_artifact_bytes)?;
        if bytes.len() > self.max_artifact_bytes || bytes.len() != declared_size {
            return Err(UpdateError::ArtifactTooLarge {
                max_bytes: self.max_artifact_bytes,
            });
        }
        if sha256_hex(&bytes) != candidate.sha256() {
            return Err(UpdateError::ChecksumMismatch);
        }
        self.authenticity.verify(&candidate)?;
        Ok(VerifiedArtifact {
            descriptor: candidate,
            bytes,
        })
    }

    fn stage_and_activate(
        &self,
        verified: VerifiedArtifact,
        faults: &dyn UpdateFaultInjector,
    ) -> UpdateResult<ActivatedArtifact> {
        let staged = self.store.stage(&verified.descriptor, &verified.bytes)?;
        faults.check(UpdateFaultPoint::AfterStaging)?;
        let receipt = self.store.activate(staged)?;
        Ok(ActivatedArtifact {
            descriptor: verified.descriptor,
            receipt,
        })
    }

    fn finish_activation(
        &self,
        activated: ActivatedArtifact,
        faults: &dyn UpdateFaultInjector,
    ) -> UpdateResult<UpdateOutcome> {
        let health = self.health.ok_or_else(|| {
            UpdateError::Health("activation health check is not configured".to_string())
        })?;
        let result = faults
            .check(UpdateFaultPoint::AfterActivation)
            .and_then(|_| health.check(&activated.descriptor))
            .and_then(|_| faults.check(UpdateFaultPoint::BeforeCommit))
            .and_then(|_| self.store.commit(&activated.receipt));
        match result {
            Ok(()) => Ok(UpdateOutcome::Applied(activated.descriptor)),
            Err(cause) => self.rollback_after_failure(activated, cause),
        }
    }

    fn rollback_after_failure(
        &self,
        activated: ActivatedArtifact,
        cause: UpdateError,
    ) -> UpdateResult<UpdateOutcome> {
        match self.store.rollback(&activated.receipt) {
            Ok(()) => Ok(UpdateOutcome::RolledBack {
                attempted: activated.descriptor,
                reason: cause.to_string(),
            }),
            Err(rollback) => Err(UpdateError::RollbackFailed {
                cause: cause.to_string(),
                rollback: rollback.to_string(),
            }),
        }
    }
}
