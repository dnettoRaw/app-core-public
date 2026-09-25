//! Bounded, redacted diagnostics for application support bundles.

use appcore_core::redact_text_with_limit;
use serde::{Deserialize, Serialize};

use crate::{AppError, AppResult};

/// Maximum UTF-8 bytes retained in a diagnostic string.
pub const MAX_DIAGNOSTIC_TEXT_BYTES: usize = 256;
/// Maximum recent errors retained by one bundle.
pub const MAX_DIAGNOSTIC_ERRORS: usize = 32;
/// Maximum UTF-8 bytes retained in a component status.
pub const MAX_DIAGNOSTIC_STATUS_BYTES: usize = 128;

/// Stable schema marker for the common SDK diagnostic bundle.
pub const DIAGNOSTIC_BUNDLE_SCHEMA_V1: &str = "appcore.sdk.diagnostic.v1";

/// Coarse status for one optional Runtime component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticStatus {
    /// The component is operating normally.
    Healthy,
    /// The component is available with reduced guarantees.
    Degraded,
    /// The component is not configured or not running.
    Unavailable,
    /// No observation was supplied by the owning deployment.
    Unknown,
}

/// Bounded status and safe detail for Gateway, sync or update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticComponent {
    /// Coarse component state.
    pub status: DiagnosticStatus,
    /// Optional redacted, bounded detail.
    pub detail: Option<String>,
}

impl DiagnosticComponent {
    fn new(status: DiagnosticStatus, detail: Option<String>) -> Self {
        Self {
            status,
            detail: detail.map(|value| redact_diagnostic_text(&value, MAX_DIAGNOSTIC_STATUS_BYTES)),
        }
    }
}

/// One redacted recent error suitable for support diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticError {
    /// Stable owner or subsystem label.
    pub source: String,
    /// Redacted error summary.
    pub message: String,
}

/// Privacy declaration attached to every diagnostic bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticPrivacy {
    /// Whether the caller explicitly opted into sensitive diagnostics.
    pub sensitive_opt_in: bool,
    /// Whether secret values are excluded from this bundle.
    pub secrets_excluded: bool,
    /// Whether paths and free-form error text were redacted.
    pub paths_and_text_redacted: bool,
}

/// Common bounded support bundle shared by SDK applications.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticBundleV1 {
    /// Stable schema marker.
    pub schema: String,
    /// Application identifier supplied by the application.
    pub application: String,
    /// Deployment environment label, never a secret or credential.
    pub environment: String,
    /// Coarse host platform label supplied by the deployment.
    pub platform: String,
    /// Application version label.
    pub application_version: String,
    /// SDK version used to create the bundle.
    pub sdk_version: String,
    /// Runtime contract version label.
    pub runtime_version: String,
    /// Protocol version label.
    pub protocol_version: String,
    /// Non-reversible storage identity supplied by the storage owner.
    pub storage_fingerprint: Option<String>,
    /// Gateway state, when that capability is deployed.
    pub gateway: DiagnosticComponent,
    /// Sync state, when that capability is deployed.
    pub sync: DiagnosticComponent,
    /// Update state, when that capability is deployed.
    pub update: DiagnosticComponent,
    /// Most recent safe errors, bounded and redacted.
    pub recent_errors: Vec<DiagnosticError>,
    /// Explicit privacy declaration.
    pub privacy: DiagnosticPrivacy,
}

impl DiagnosticBundleV1 {
    /// Serializes the bundle as bounded, redacted JSON.
    pub fn to_json(&self) -> AppResult<String> {
        serde_json::to_string(self).map_err(|error| AppError::Diagnostics(error.to_string()))
    }
}

/// Builder for a common redacted diagnostic bundle.
#[derive(Debug, Clone)]
pub struct DiagnosticBundleBuilder {
    application: String,
    environment: String,
    platform: String,
    application_version: String,
    sdk_version: String,
    runtime_version: String,
    protocol_version: String,
    storage_fingerprint: Option<String>,
    gateway: DiagnosticComponent,
    sync: DiagnosticComponent,
    update: DiagnosticComponent,
    recent_errors: Vec<DiagnosticError>,
    sensitive_opt_in: bool,
}

impl DiagnosticBundleBuilder {
    /// Creates a builder with safe unknown component defaults.
    pub fn new(application: impl Into<String>) -> Self {
        Self {
            application: application.into(),
            environment: "unknown".to_owned(),
            platform: "unknown".to_owned(),
            application_version: "unknown".to_owned(),
            sdk_version: env!("CARGO_PKG_VERSION").to_owned(),
            runtime_version: "unknown".to_owned(),
            protocol_version: "unknown".to_owned(),
            storage_fingerprint: None,
            gateway: DiagnosticComponent::new(DiagnosticStatus::Unknown, None),
            sync: DiagnosticComponent::new(DiagnosticStatus::Unknown, None),
            update: DiagnosticComponent::new(DiagnosticStatus::Unknown, None),
            recent_errors: Vec::new(),
            sensitive_opt_in: false,
        }
    }

    /// Sets the deployment environment label.
    pub fn environment(mut self, value: impl Into<String>) -> Self {
        self.environment = value.into();
        self
    }

    /// Sets the environment from the unified non-secret SDK profile.
    pub fn environment_profile(mut self, profile: &crate::EnvironmentProfile) -> Self {
        self.environment = profile.diagnostic_label();
        self
    }

    /// Sets the coarse platform label.
    pub fn platform(mut self, value: impl Into<String>) -> Self {
        self.platform = value.into();
        self
    }

    /// Sets application, Runtime and protocol version labels.
    pub fn versions(
        mut self,
        application: impl Into<String>,
        runtime: impl Into<String>,
        protocol: impl Into<String>,
    ) -> Self {
        self.application_version = application.into();
        self.runtime_version = runtime.into();
        self.protocol_version = protocol.into();
        self
    }

    /// Sets a non-reversible storage fingerprint; raw paths are never accepted.
    pub fn storage_fingerprint(mut self, value: impl Into<String>) -> Self {
        self.storage_fingerprint = Some(value.into());
        self
    }

    /// Sets the Gateway observation.
    pub fn gateway(mut self, status: DiagnosticStatus, detail: Option<String>) -> Self {
        self.gateway = DiagnosticComponent::new(status, detail);
        self
    }

    /// Sets the sync observation.
    pub fn sync(mut self, status: DiagnosticStatus, detail: Option<String>) -> Self {
        self.sync = DiagnosticComponent::new(status, detail);
        self
    }

    /// Sets the update observation.
    pub fn update(mut self, status: DiagnosticStatus, detail: Option<String>) -> Self {
        self.update = DiagnosticComponent::new(status, detail);
        self
    }

    /// Adds one error after redaction and bounded truncation.
    pub fn recent_error(mut self, source: impl Into<String>, message: impl Into<String>) -> Self {
        if self.recent_errors.len() < MAX_DIAGNOSTIC_ERRORS {
            self.recent_errors.push(DiagnosticError {
                source: redact_diagnostic_text(&source.into(), MAX_DIAGNOSTIC_TEXT_BYTES),
                message: redact_diagnostic_text(&message.into(), MAX_DIAGNOSTIC_TEXT_BYTES),
            });
        }
        self
    }

    /// Records an explicit caller opt-in without including secret values.
    pub fn sensitive_opt_in(mut self, enabled: bool) -> Self {
        self.sensitive_opt_in = enabled;
        self
    }

    /// Validates and builds the redacted V1 bundle.
    pub fn build(self) -> AppResult<DiagnosticBundleV1> {
        Ok(DiagnosticBundleV1 {
            schema: DIAGNOSTIC_BUNDLE_SCHEMA_V1.to_owned(),
            application: bounded_required(self.application, "application")?,
            environment: bounded_required(self.environment, "environment")?,
            platform: bounded_required(self.platform, "platform")?,
            application_version: bounded_required(self.application_version, "application_version")?,
            sdk_version: bounded_required(self.sdk_version, "sdk_version")?,
            runtime_version: bounded_required(self.runtime_version, "runtime_version")?,
            protocol_version: bounded_required(self.protocol_version, "protocol_version")?,
            storage_fingerprint: self
                .storage_fingerprint
                .map(|value| bounded_required(value, "storage_fingerprint"))
                .transpose()?,
            gateway: self.gateway,
            sync: self.sync,
            update: self.update,
            recent_errors: self.recent_errors,
            privacy: DiagnosticPrivacy {
                sensitive_opt_in: self.sensitive_opt_in,
                secrets_excluded: true,
                paths_and_text_redacted: true,
            },
        })
    }
}

fn bounded_required(value: String, field: &'static str) -> AppResult<String> {
    if value.trim().is_empty() || value.len() > MAX_DIAGNOSTIC_TEXT_BYTES {
        return Err(AppError::Diagnostics(format!("invalid diagnostic {field}")));
    }
    Ok(redact_diagnostic_text(&value, MAX_DIAGNOSTIC_TEXT_BYTES))
}

fn redact_diagnostic_text(value: &str, max_bytes: usize) -> String {
    let redacted = redact_text_with_limit(value, max_bytes);
    redacted
        .split_whitespace()
        .map(|token| {
            ["/Users/", "/home/", "/var/", ":\\"]
                .iter()
                .find_map(|prefix| token.find(prefix).map(|index| (index, *prefix)))
                .map_or_else(
                    || token.to_owned(),
                    |(index, _prefix)| format!("{}[PATH_REDACTED]", &token[..index]),
                )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_is_bounded_redacted_and_serializable() {
        let bundle = DiagnosticBundleBuilder::new("demo")
            .environment("qa")
            .platform("macos")
            .storage_fingerprint("sha256:abc")
            .gateway(
                DiagnosticStatus::Healthy,
                Some("token=[REDACTED] path=/var/app".to_owned()),
            )
            .recent_error("sync", "password=[REDACTED] path=/var/data")
            .build()
            .unwrap();
        let json = bundle.to_json().unwrap();
        assert!(json.contains(DIAGNOSTIC_BUNDLE_SCHEMA_V1));
        assert!(!json.contains("/var/"));
        assert!(json.contains("PATH_REDACTED"));
        assert!(bundle.privacy.secrets_excluded);
    }

    #[test]
    fn bundle_rejects_empty_required_fields_and_bounds_errors() {
        assert!(DiagnosticBundleBuilder::new("").build().is_err());
        let bundle = (0..MAX_DIAGNOSTIC_ERRORS + 4)
            .fold(DiagnosticBundleBuilder::new("demo"), |builder, index| {
                builder.recent_error("test", index.to_string())
            })
            .build()
            .unwrap();
        assert_eq!(bundle.recent_errors.len(), MAX_DIAGNOSTIC_ERRORS);
    }
}
