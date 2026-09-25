//! Managed mobile update policy and compatibility gates.

use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

use crate::{ArtifactTarget, UpdateError, UpdateResult};

/// Mobile update action owned by the deployment or platform distribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MobileUpdateAction {
    /// The application may replace its own artifact under deployment policy.
    SelfReplaceAllowed,
    /// The user must update through the platform store.
    StoreUpdateRequired,
    /// The managed device administrator must authorize the update.
    MdmUpdateRequired,
    /// A deployment-assisted sideload path may be presented by the owner.
    SideloadAssisted,
    /// No supported update path exists for this target.
    Unsupported,
}

/// Request context used to evaluate one mobile update offer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MobileUpdateRequest {
    /// Currently installed application version.
    pub current_version: String,
    /// Candidate application version, when one is available.
    pub available_version: Option<String>,
    /// Cluster version currently serving the application.
    pub cluster_version: String,
    /// Protocol used by the installed application.
    pub protocol_version: String,
    /// Platform target selected by the deployment.
    pub target: ArtifactTarget,
}

/// Result of the bounded mobile update compatibility gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum MobileUpdateDecision {
    /// No newer compatible candidate is available.
    NoUpdate,
    /// A newer candidate is available through the declared action.
    Available {
        /// Action the owning platform/deployment must perform.
        action: MobileUpdateAction,
        /// Candidate version to announce.
        version: String,
    },
    /// The installed client must update before protocol use continues.
    ProtocolBlocked {
        /// Protocol required by the deployment.
        required: String,
        /// Protocol currently presented by the client.
        current: String,
    },
    /// The connected cluster is below the policy minimum.
    ClusterTooOld {
        /// Minimum cluster requirement.
        minimum: String,
        /// Current cluster version.
        current: String,
    },
}

/// Explicit mobile update policy with no store or installer implementation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MobileUpdatePolicy {
    /// Platform/deployment-owned action for a compatible candidate.
    pub action: MobileUpdateAction,
    /// Minimum cluster semantic-version requirement.
    pub minimum_cluster_version: String,
    /// Exact protocol required by the deployed Runtime.
    pub required_protocol_version: String,
}

impl MobileUpdatePolicy {
    /// Creates a policy after validating action and compatibility bounds.
    pub fn new(
        action: MobileUpdateAction,
        minimum_cluster_version: impl Into<String>,
        required_protocol_version: impl Into<String>,
    ) -> UpdateResult<Self> {
        let policy = Self {
            action,
            minimum_cluster_version: minimum_cluster_version.into(),
            required_protocol_version: required_protocol_version.into(),
        };
        policy.validate()?;
        Ok(policy)
    }

    /// Validates policy metadata without contacting a store or device manager.
    pub fn validate(&self) -> UpdateResult<()> {
        VersionReq::parse(&self.minimum_cluster_version).map_err(|error| {
            UpdateError::Incompatible(format!("invalid mobile cluster requirement: {error}"))
        })?;
        validate_protocol(&self.required_protocol_version)
    }

    /// Evaluates one candidate while preserving platform-owned update actions.
    pub fn evaluate(&self, request: &MobileUpdateRequest) -> UpdateResult<MobileUpdateDecision> {
        self.validate()?;
        request.target_validate()?;
        validate_version("current_version", &request.current_version)?;
        validate_version("cluster_version", &request.cluster_version)?;
        validate_protocol(&request.protocol_version)?;
        if request.protocol_version != self.required_protocol_version {
            return Ok(MobileUpdateDecision::ProtocolBlocked {
                required: self.required_protocol_version.clone(),
                current: request.protocol_version.clone(),
            });
        }
        let cluster = Version::parse(&request.cluster_version).map_err(|error| {
            UpdateError::Incompatible(format!("invalid mobile cluster version: {error}"))
        })?;
        let minimum = VersionReq::parse(&self.minimum_cluster_version).map_err(|error| {
            UpdateError::Incompatible(format!("invalid mobile cluster requirement: {error}"))
        })?;
        if !minimum.matches(&cluster) {
            return Ok(MobileUpdateDecision::ClusterTooOld {
                minimum: self.minimum_cluster_version.clone(),
                current: request.cluster_version.clone(),
            });
        }
        let Some(version) = &request.available_version else {
            return Ok(MobileUpdateDecision::NoUpdate);
        };
        validate_version("available_version", version)?;
        let offered = Version::parse(version).map_err(|error| {
            UpdateError::Incompatible(format!("invalid mobile available version: {error}"))
        })?;
        let current = Version::parse(&request.current_version).map_err(|error| {
            UpdateError::Incompatible(format!("invalid mobile current version: {error}"))
        })?;
        if offered <= current {
            return Ok(MobileUpdateDecision::NoUpdate);
        }
        Ok(MobileUpdateDecision::Available {
            action: self.action,
            version: version.clone(),
        })
    }
}

trait MobileTargetValidation {
    fn target_validate(&self) -> UpdateResult<()>;
}

impl MobileTargetValidation for MobileUpdateRequest {
    fn target_validate(&self) -> UpdateResult<()> {
        if self.target.os().trim().is_empty()
            || self.target.architecture().trim().is_empty()
            || self.target.format().trim().is_empty()
        {
            return Err(UpdateError::InvalidArtifact(
                "mobile target fields must be present".to_owned(),
            ));
        }
        Ok(())
    }
}

fn validate_protocol(value: &str) -> UpdateResult<()> {
    if value.is_empty() || value.len() > 32 || value.chars().any(char::is_control) {
        return Err(UpdateError::Incompatible(
            "mobile protocol version is invalid".to_owned(),
        ));
    }
    Ok(())
}

fn validate_version(name: &str, value: &str) -> UpdateResult<()> {
    if value.len() > 64 {
        return Err(UpdateError::Incompatible(format!("{name} is too long")));
    }
    Version::parse(value)
        .map(|_| ())
        .map_err(|error| UpdateError::Incompatible(format!("invalid {name}: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(protocol: &str, cluster: &str, available: Option<&str>) -> MobileUpdateRequest {
        MobileUpdateRequest {
            current_version: "1.0.0".to_owned(),
            available_version: available.map(ToOwned::to_owned),
            cluster_version: cluster.to_owned(),
            protocol_version: protocol.to_owned(),
            target: ArtifactTarget::new("ios", "arm64", "store").unwrap(),
        }
    }

    #[test]
    fn announces_newer_offer_without_recommending_a_bypass() {
        let policy =
            MobileUpdatePolicy::new(MobileUpdateAction::StoreUpdateRequired, ">=1.4.0", "1")
                .unwrap();
        assert_eq!(
            policy
                .evaluate(&request("1", "1.5.0", Some("1.1.0")))
                .unwrap(),
            MobileUpdateDecision::Available {
                action: MobileUpdateAction::StoreUpdateRequired,
                version: "1.1.0".to_owned(),
            }
        );
    }

    #[test]
    fn blocks_old_protocol_and_cluster_before_offer() {
        let policy =
            MobileUpdatePolicy::new(MobileUpdateAction::SelfReplaceAllowed, ">=2.0.0", "2")
                .unwrap();
        assert!(matches!(
            policy.evaluate(&request("1", "2.0.0", Some("2.0.0"))),
            Ok(MobileUpdateDecision::ProtocolBlocked { .. })
        ));
        assert!(matches!(
            policy.evaluate(&request("2", "1.9.0", Some("2.0.0"))),
            Ok(MobileUpdateDecision::ClusterTooOld { .. })
        ));
    }
}
