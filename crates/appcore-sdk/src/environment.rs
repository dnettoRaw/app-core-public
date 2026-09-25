//! Unified, non-secret SDK environment identity.

use serde::{Deserialize, Serialize};

use crate::{AppError, AppResult};

/// Maximum bytes retained by one environment label.
pub const MAX_ENVIRONMENT_LABEL_BYTES: usize = 64;

/// Deployment stage shown in diagnostics and operational policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentStage {
    /// Local developer environment.
    Dev,
    /// Quality-assurance environment.
    Qa,
    /// Production environment.
    Production,
}

/// Whether artifacts are local-development or release-managed outputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseKind {
    /// Local or development artifact.
    Local,
    /// Published, release-managed artifact.
    Release,
}

/// Runtime surface hosting the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeSurface {
    /// Desktop-hosted application.
    Desktop,
    /// Dedicated synchronization node.
    SyncNode,
    /// Mobile-hosted application.
    Mobile,
}

/// Local or cluster deployment topology.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentTopology {
    /// Offline or standalone deployment.
    Standalone,
    /// Cluster-coordinated deployment.
    Cluster,
}

/// Complete non-secret identity used by SDK applications and diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentProfile {
    /// Dev, QA or production stage.
    pub stage: EnvironmentStage,
    /// Local or release artifact track.
    pub release: ReleaseKind,
    /// Desktop, sync-node or mobile surface.
    pub surface: RuntimeSurface,
    /// Standalone or cluster topology.
    pub topology: EnvironmentTopology,
    /// Optional opaque cluster label.
    pub cluster: Option<String>,
    /// Optional opaque tenant label.
    pub tenant: Option<String>,
    /// Explicit storage namespace.
    pub storage_namespace: String,
    /// Explicit update channel.
    pub update_channel: String,
}

impl EnvironmentProfile {
    /// Creates a profile and validates all labels and topology requirements.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        stage: EnvironmentStage,
        release: ReleaseKind,
        surface: RuntimeSurface,
        topology: EnvironmentTopology,
        cluster: Option<String>,
        tenant: Option<String>,
        storage_namespace: impl Into<String>,
        update_channel: impl Into<String>,
    ) -> AppResult<Self> {
        let profile = Self {
            stage,
            release,
            surface,
            topology,
            cluster,
            tenant,
            storage_namespace: storage_namespace.into(),
            update_channel: update_channel.into(),
        };
        profile.validate()?;
        Ok(profile)
    }

    /// Validates labels, cluster/tenant topology and release-stage coherence.
    pub fn validate(&self) -> AppResult<()> {
        bounded_label(&self.storage_namespace, "storage_namespace")?;
        bounded_label(&self.update_channel, "update_channel")?;
        if let Some(cluster) = &self.cluster {
            bounded_label(cluster, "cluster")?;
        }
        if let Some(tenant) = &self.tenant {
            bounded_label(tenant, "tenant")?;
        }
        if self.topology == EnvironmentTopology::Cluster && self.cluster.is_none() {
            return Err(AppError::Environment(
                "cluster topology requires a cluster label".to_owned(),
            ));
        }
        if self.release == ReleaseKind::Release && self.stage == EnvironmentStage::Dev {
            return Err(AppError::Environment(
                "development cannot use the release artifact track".to_owned(),
            ));
        }
        Ok(())
    }

    /// Requires two explicit namespaces to differ before a release is promoted.
    pub fn validate_namespace_separation(
        non_release_namespace: &str,
        release_namespace: &str,
    ) -> AppResult<()> {
        bounded_label(non_release_namespace, "non_release_namespace")?;
        bounded_label(release_namespace, "release_namespace")?;
        if non_release_namespace == release_namespace {
            return Err(AppError::Environment(
                "development and release namespaces must differ".to_owned(),
            ));
        }
        Ok(())
    }

    /// Returns a compact diagnostic label containing no secret or path value.
    pub fn diagnostic_label(&self) -> String {
        format!(
            "{}/{}/{}/{}/{}/{}",
            stage_label(self.stage),
            release_label(self.release),
            surface_label(self.surface),
            topology_label(self.topology),
            self.storage_namespace,
            self.update_channel
        )
    }
}

fn bounded_label(value: &str, field: &str) -> AppResult<()> {
    if value.trim().is_empty()
        || value.len() > MAX_ENVIRONMENT_LABEL_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(AppError::Environment(format!(
            "invalid environment {field}"
        )));
    }
    Ok(())
}

const fn stage_label(value: EnvironmentStage) -> &'static str {
    match value {
        EnvironmentStage::Dev => "dev",
        EnvironmentStage::Qa => "qa",
        EnvironmentStage::Production => "production",
    }
}

const fn release_label(value: ReleaseKind) -> &'static str {
    match value {
        ReleaseKind::Local => "local",
        ReleaseKind::Release => "release",
    }
}

const fn surface_label(value: RuntimeSurface) -> &'static str {
    match value {
        RuntimeSurface::Desktop => "desktop",
        RuntimeSurface::SyncNode => "sync-node",
        RuntimeSurface::Mobile => "mobile",
    }
}

const fn topology_label(value: EnvironmentTopology) -> &'static str {
    match value {
        EnvironmentTopology::Standalone => "standalone",
        EnvironmentTopology::Cluster => "cluster",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_is_explicit_and_diagnostic_label_is_non_secret() {
        let profile = EnvironmentProfile::new(
            EnvironmentStage::Qa,
            ReleaseKind::Release,
            RuntimeSurface::Mobile,
            EnvironmentTopology::Cluster,
            Some("cluster-a".to_owned()),
            Some("tenant-a".to_owned()),
            "qa-storage",
            "beta",
        )
        .unwrap();
        assert_eq!(
            profile.diagnostic_label(),
            "qa/release/mobile/cluster/qa-storage/beta"
        );
    }

    #[test]
    fn release_and_non_release_namespaces_must_differ() {
        assert!(EnvironmentProfile::validate_namespace_separation("dev", "release").is_ok());
        assert!(EnvironmentProfile::validate_namespace_separation("same", "same").is_err());
        assert!(EnvironmentProfile::new(
            EnvironmentStage::Dev,
            ReleaseKind::Release,
            RuntimeSurface::Desktop,
            EnvironmentTopology::Standalone,
            None,
            None,
            "dev",
            "local",
        )
        .is_err());
    }
}
