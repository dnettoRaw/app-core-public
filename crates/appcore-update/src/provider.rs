// =============================================================================
//        #######
//     ###       ###     F: provider.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 13:24:05 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Defines bounded provider contracts and behavior for this crate.

use crate::filesystem::{open_regular_file, read_regular_file_bounded};
use crate::{ArtifactDescriptor, UpdateError, UpdateResult};
use appcore_contracts::ApplicationId;
use appcore_provider::{
    ProviderContext, ProviderError, ProviderFactory, ProviderResult, ProviderRole, SecretProvider,
};
use semver::Version;
use serde::de::{SeqAccess, Visitor};
use serde::Deserializer as _;
use std::fmt;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(crate) const FILE_UPDATE_INDEX_MAX_BYTES: usize = 1_048_576;
const FILE_UPDATE_INDEX_BUFFER_BYTES: usize = 16 * 1024;

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

    fn latest_from_index(
        &self,
        request: &UpdateRequest,
        current: &Version,
    ) -> UpdateResult<Option<ArtifactDescriptor>> {
        let file = open_regular_file(&self.index_path)
            .map_err(|error| UpdateError::Provider(error.to_string()))?;
        let declared_length = file
            .metadata()
            .map_err(|error| UpdateError::Provider(error.to_string()))?
            .len();
        select_index(file, declared_length, request, current)
    }
}

pub(crate) fn select_index(
    reader: impl Read,
    declared_length: u64,
    request: &UpdateRequest,
    current: &Version,
) -> UpdateResult<Option<ArtifactDescriptor>> {
    let max_bytes = FILE_UPDATE_INDEX_MAX_BYTES as u64;
    if declared_length > max_bytes {
        return Err(index_size_error());
    }
    let mut reader =
        BufReader::with_capacity(FILE_UPDATE_INDEX_BUFFER_BYTES, reader.take(max_bytes + 1));
    let mut deserializer = serde_json::Deserializer::from_reader(&mut reader);
    let selection = deserializer
        .deserialize_seq(LatestIndexVisitor { request, current })
        .map_err(|error| UpdateError::Provider(error.to_string()))?;
    deserializer
        .end()
        .map_err(|error| UpdateError::Provider(error.to_string()))?;
    if reader.get_ref().limit() == 0 {
        return Err(index_size_error());
    }
    selection
}

struct LatestIndexVisitor<'a> {
    request: &'a UpdateRequest,
    current: &'a Version,
}

impl<'de> Visitor<'de> for LatestIndexVisitor<'_> {
    type Value = UpdateResult<Option<ArtifactDescriptor>>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an array of update artifact descriptors")
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut selected = None;
        let mut validation_error = None;
        while let Some(artifact) = sequence.next_element::<ArtifactDescriptor>()? {
            if validation_error.is_some() {
                continue;
            }
            if let Err(error) = artifact.validate() {
                validation_error = Some(error);
                continue;
            }
            consider_candidate(artifact, self.request, self.current, &mut selected);
        }
        if let Some(error) = validation_error {
            return Ok(Err(error));
        }
        Ok(Ok(selected.map(|(_, artifact)| artifact)))
    }
}

fn consider_candidate(
    artifact: ArtifactDescriptor,
    request: &UpdateRequest,
    current: &Version,
    selected: &mut Option<(Version, ArtifactDescriptor)>,
) {
    if artifact.application_id() != &request.application_id || artifact.channel() != request.channel
    {
        return;
    }
    let Ok(version) = Version::parse(artifact.application_version()) else {
        return;
    };
    if version <= *current
        || selected
            .as_ref()
            .is_some_and(|(selected_version, _)| version <= *selected_version)
    {
        return;
    }
    *selected = Some((version, artifact));
}

fn index_size_error() -> UpdateError {
    UpdateError::Provider("update index exceeds configured read limit".to_string())
}

impl UpdateProvider for FileUpdateProvider {
    fn latest(&self, request: &UpdateRequest) -> UpdateResult<Option<ArtifactDescriptor>> {
        let current = Version::parse(&request.current_version).map_err(|error| {
            UpdateError::Provider(format!("invalid installed application version: {error}"))
        })?;
        self.latest_from_index(request, &current)
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
