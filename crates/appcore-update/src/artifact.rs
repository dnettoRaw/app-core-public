// =============================================================================
//        #######
//     ###       ###     F: artifact.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 13:45:20 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use crate::{UpdateError, UpdateResult};
use appcore_contracts::{ApplicationId, BuildId};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

/// Immutable application artifact published by an update provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactDescriptor {
    application_id: ApplicationId,
    application_version: String,
    build_id: BuildId,
    channel: String,
    runtime_requirement: String,
    protocol_version: String,
    artifact_reference: String,
    sha256: String,
    size_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    signing_key_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ed25519_signature: Option<String>,
}

impl ArtifactDescriptor {
    /// Creates and validates an immutable artifact descriptor.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        application_id: ApplicationId,
        application_version: impl Into<String>,
        build_id: BuildId,
        channel: impl Into<String>,
        runtime_requirement: impl Into<String>,
        protocol_version: impl Into<String>,
        artifact_reference: impl Into<String>,
        sha256: impl Into<String>,
        size_bytes: u64,
    ) -> UpdateResult<Self> {
        let descriptor = Self {
            application_id,
            application_version: application_version.into(),
            build_id,
            channel: channel.into(),
            runtime_requirement: runtime_requirement.into(),
            protocol_version: protocol_version.into(),
            artifact_reference: artifact_reference.into(),
            sha256: sha256.into(),
            size_bytes,
            signing_key_id: None,
            ed25519_signature: None,
        };
        descriptor.validate()?;
        Ok(descriptor)
    }

    /// Adds the Ed25519 signing key identity and detached signature.
    pub fn with_ed25519_signature(
        mut self,
        signing_key_id: impl Into<String>,
        signature: impl Into<String>,
    ) -> UpdateResult<Self> {
        self.signing_key_id = Some(signing_key_id.into());
        self.ed25519_signature = Some(signature.into());
        self.validate()?;
        Ok(self)
    }

    /// Returns the application identity.
    pub fn application_id(&self) -> &ApplicationId {
        &self.application_id
    }

    /// Returns the semantic application version.
    pub fn application_version(&self) -> &str {
        &self.application_version
    }

    /// Returns the immutable build identity.
    pub fn build_id(&self) -> &BuildId {
        &self.build_id
    }

    /// Returns the publication channel.
    pub fn channel(&self) -> &str {
        &self.channel
    }

    /// Returns the runtime semantic-version requirement.
    pub fn runtime_requirement(&self) -> &str {
        &self.runtime_requirement
    }

    /// Returns the required distributed protocol version.
    pub fn protocol_version(&self) -> &str {
        &self.protocol_version
    }

    /// Returns the opaque provider reference used to fetch bytes.
    pub fn artifact_reference(&self) -> &str {
        &self.artifact_reference
    }

    /// Returns the lowercase SHA-256 checksum.
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    /// Returns the declared artifact size.
    pub fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    /// Returns the configured signing key identity.
    pub fn signing_key_id(&self) -> Option<&str> {
        self.signing_key_id.as_deref()
    }

    /// Returns the lowercase hexadecimal Ed25519 signature.
    pub fn ed25519_signature(&self) -> Option<&str> {
        self.ed25519_signature.as_deref()
    }

    /// Checks runtime and protocol compatibility.
    pub fn ensure_compatible(
        &self,
        runtime_version: &str,
        protocol_version: &str,
    ) -> UpdateResult<()> {
        let runtime = Version::parse(runtime_version).map_err(|error| {
            UpdateError::Incompatible(format!("invalid runtime version: {error}"))
        })?;
        let requirement = VersionReq::parse(&self.runtime_requirement).map_err(|error| {
            UpdateError::InvalidArtifact(format!("invalid runtime requirement: {error}"))
        })?;
        if !requirement.matches(&runtime) {
            return Err(UpdateError::Incompatible(format!(
                "runtime {runtime} does not satisfy {}",
                self.runtime_requirement
            )));
        }
        if self.protocol_version != protocol_version {
            return Err(UpdateError::Incompatible(format!(
                "protocol {} is required, host provides {protocol_version}",
                self.protocol_version
            )));
        }
        Ok(())
    }

    pub(crate) fn validate(&self) -> UpdateResult<()> {
        for (name, value, max) in [
            ("application_version", self.application_version.as_str(), 64),
            ("channel", self.channel.as_str(), 64),
            (
                "runtime_requirement",
                self.runtime_requirement.as_str(),
                128,
            ),
            ("protocol_version", self.protocol_version.as_str(), 64),
            (
                "artifact_reference",
                self.artifact_reference.as_str(),
                2_048,
            ),
        ] {
            if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
                return Err(UpdateError::InvalidArtifact(format!(
                    "{name} is empty, too long or contains control characters"
                )));
            }
        }
        Version::parse(&self.application_version).map_err(|error| {
            UpdateError::InvalidArtifact(format!("invalid application version: {error}"))
        })?;
        VersionReq::parse(&self.runtime_requirement).map_err(|error| {
            UpdateError::InvalidArtifact(format!("invalid runtime requirement: {error}"))
        })?;
        if self.size_bytes == 0 {
            return Err(UpdateError::InvalidArtifact(
                "size_bytes must be greater than zero".to_string(),
            ));
        }
        if self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(UpdateError::InvalidArtifact(
                "sha256 must be 64 lowercase hexadecimal characters".to_string(),
            ));
        }
        match (&self.signing_key_id, &self.ed25519_signature) {
            (None, None) => {}
            (Some(key_id), Some(signature)) => {
                if key_id.trim().is_empty()
                    || key_id.len() > 128
                    || key_id.chars().any(char::is_control)
                {
                    return Err(UpdateError::InvalidArtifact(
                        "signing_key_id is empty, too long or contains control characters"
                            .to_string(),
                    ));
                }
                if signature.len() != 128
                    || !signature
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
                {
                    return Err(UpdateError::InvalidArtifact(
                        "ed25519_signature must be 128 lowercase hexadecimal characters"
                            .to_string(),
                    ));
                }
            }
            _ => {
                return Err(UpdateError::InvalidArtifact(
                    "signing_key_id and ed25519_signature must be provided together".to_string(),
                ));
            }
        }
        Ok(())
    }
}
