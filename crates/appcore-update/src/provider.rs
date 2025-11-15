// =============================================================================
//        #######
//     ###       ###     F: provider.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 13:24:05 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use crate::filesystem::read_regular_file_bounded;
use crate::{ArtifactDescriptor, UpdateError, UpdateResult};
use appcore_contracts::ApplicationId;
use appcore_provider::{
    ProviderContext, ProviderError, ProviderFactory, ProviderResult, ProviderRole, SecretProvider,
};
use semver::Version;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Query used to select an update candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateRequest {
    /// Installed application identity.
    pub application_id: ApplicationId,
    /// Current semantic application version.
    pub current_version: String,
    /// Selected update channel.
    pub channel: String,
}

/// Provider contract for listing and fetching opaque application artifacts.
pub trait UpdateProvider: Send + Sync {
    /// Returns the newest eligible artifact, or `None` when no update exists.
    fn latest(&self, request: &UpdateRequest) -> UpdateResult<Option<ArtifactDescriptor>>;

    /// Fetches complete artifact bytes while respecting `max_bytes`.
    fn fetch(&self, artifact: &ArtifactDescriptor, max_bytes: usize) -> UpdateResult<Vec<u8>>;
}

/// Shared update provider interface produced by deployment factories.
pub type SharedUpdateProvider = Arc<dyn UpdateProvider>;

/// Provider ID for the local-first JSON index and file artifact adapter.
pub const FILE_UPDATE_PROVIDER_ID: &str = "file-update";

/// Local-first update provider backed by a bounded JSON artifact index.
#[derive(Debug, Clone)]
pub struct FileUpdateProvider {
    index_path: PathBuf,
}

impl FileUpdateProvider {
    /// Creates a provider from an installation-owned index path.
    pub fn new(index_path: impl Into<PathBuf>) -> Self {
        Self {
            index_path: index_path.into(),
        }
    }

    fn read_index(&self) -> UpdateResult<Vec<ArtifactDescriptor>> {
        let bytes = read_provider_file(&self.index_path, 1_048_576)?;
        let artifacts: Vec<ArtifactDescriptor> = serde_json::from_slice(&bytes)
            .map_err(|error| UpdateError::Provider(error.to_string()))?;
        for artifact in &artifacts {
            artifact.validate()?;
        }
        Ok(artifacts)
    }
}

impl UpdateProvider for FileUpdateProvider {
    fn latest(&self, request: &UpdateRequest) -> UpdateResult<Option<ArtifactDescriptor>> {
        let current = Version::parse(&request.current_version).map_err(|error| {
            UpdateError::Provider(format!("invalid installed application version: {error}"))
        })?;
        let mut eligible = self
            .read_index()?
            .into_iter()
            .filter(|artifact| {
                artifact.application_id() == &request.application_id
                    && artifact.channel() == request.channel
            })
            .filter_map(|artifact| {
                Version::parse(artifact.application_version())
                    .ok()
                    .filter(|version| version > &current)
                    .map(|version| (version, artifact))
            })
            .collect::<Vec<_>>();
        eligible.sort_by(|left, right| right.0.cmp(&left.0));
        Ok(eligible.into_iter().next().map(|(_, artifact)| artifact))
    }

    fn fetch(&self, artifact: &ArtifactDescriptor, max_bytes: usize) -> UpdateResult<Vec<u8>> {
        let path = artifact
            .artifact_reference()
            .strip_prefix("file:")
            .ok_or_else(|| {
                UpdateError::Provider("file-update artifact reference must use file:".to_string())
            })?;
        match read_regular_file_bounded(Path::new(path), max_bytes) {
            Ok(bytes) => Ok(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
                Err(UpdateError::ArtifactTooLarge { max_bytes })
            }
            Err(error) => Err(UpdateError::Provider(error.to_string())),
        }
    }
}

fn read_provider_file(path: &Path, max_bytes: usize) -> UpdateResult<Vec<u8>> {
    read_regular_file_bounded(path, max_bytes)
        .map_err(|error| UpdateError::Provider(error.to_string()))
}

/// Factory for the local-first file update provider.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileUpdateProviderFactory;

impl ProviderFactory<SharedUpdateProvider> for FileUpdateProviderFactory {
    fn role(&self) -> ProviderRole {
        ProviderRole::Update
    }

    fn provider_id(&self) -> &'static str {
        FILE_UPDATE_PROVIDER_ID
    }

    fn create(
        &self,
        config: &appcore_contracts::ProviderConfig,
        _context: &ProviderContext,
        _secrets: &dyn SecretProvider,
    ) -> ProviderResult<SharedUpdateProvider> {
        let endpoint = config.endpoint().ok_or_else(|| {
            ProviderError::InvalidConfiguration(
                "file-update provider requires an index endpoint".to_string(),
            )
        })?;
        let path = endpoint.strip_prefix("file:").unwrap_or(endpoint);
        if path.trim().is_empty() {
            return Err(ProviderError::InvalidConfiguration(
                "file-update index path is empty".to_string(),
            ));
        }
        Ok(Arc::new(FileUpdateProvider::new(path)))
    }
}
