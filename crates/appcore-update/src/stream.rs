// =============================================================================
//        #######
//     ###       ###     F: stream.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/24 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/24 00:00:00 by dnettoRaw
//      ###########      S: 1.0.3-rc
// =============================================================================

//! Bounded streaming contracts for opaque artifact transfer.

use crate::filesystem::open_regular_file;
use crate::{ArtifactDescriptor, UpdateError, UpdateResult};
use sha2::{Digest, Sha256};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Conservative default maximum decoded chunk size.
pub const DEFAULT_ARTIFACT_CHUNK_BYTES: usize = 64 * 1024;

/// Transfer limits applied before reading or allocating a chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArtifactTransferPolicy {
    /// Maximum bytes accepted in one decoded chunk.
    pub max_chunk_bytes: usize,
}

impl ArtifactTransferPolicy {
    /// Creates a transfer policy with an explicit positive chunk bound.
    pub fn new(max_chunk_bytes: usize) -> UpdateResult<Self> {
        if max_chunk_bytes == 0 {
            return Err(UpdateError::Transfer(
                "max_chunk_bytes must be greater than zero".to_string(),
            ));
        }
        Ok(Self { max_chunk_bytes })
    }
}

impl Default for ArtifactTransferPolicy {
    fn default() -> Self {
        Self {
            max_chunk_bytes: DEFAULT_ARTIFACT_CHUNK_BYTES,
        }
    }
}

/// Reads bounded chunks for one immutable artifact.
pub trait ArtifactSource: Send + Sync {
    /// Reads at most `max_len` bytes beginning at `offset`.
    fn read_chunk(
        &self,
        descriptor: &ArtifactDescriptor,
        offset: u64,
        max_len: usize,
    ) -> UpdateResult<Vec<u8>>;
}

/// Receives chunks without activating or installing the artifact.
pub trait ArtifactWriter {
    /// Persists one chunk at its exact stream offset.
    fn write_chunk(&mut self, offset: u64, bytes: &[u8]) -> UpdateResult<()>;
}

/// Receives and verifies one complete artifact through a bounded stream.
pub fn receive_artifact(
    policy: &ArtifactTransferPolicy,
    descriptor: &ArtifactDescriptor,
    source: &dyn ArtifactSource,
    writer: &mut dyn ArtifactWriter,
) -> UpdateResult<()> {
    receive_artifact_from(policy, descriptor, source, writer, 0, Sha256::new())
}

pub(crate) fn receive_artifact_from(
    policy: &ArtifactTransferPolicy,
    descriptor: &ArtifactDescriptor,
    source: &dyn ArtifactSource,
    writer: &mut dyn ArtifactWriter,
    start_offset: u64,
    mut hasher: Sha256,
) -> UpdateResult<()> {
    descriptor.validate()?;
    if policy.max_chunk_bytes == 0 {
        return Err(UpdateError::Transfer(
            "max_chunk_bytes must be greater than zero".to_string(),
        ));
    }
    if start_offset > descriptor.size_bytes() {
        return Err(UpdateError::Transfer(
            "artifact start offset exceeds declared size".to_string(),
        ));
    }
    let mut offset = start_offset;
    while offset < descriptor.size_bytes() {
        let remaining = descriptor.size_bytes().checked_sub(offset).ok_or_else(|| {
            UpdateError::Transfer("artifact offset exceeds declared size".to_string())
        })?;
        let requested = usize::try_from(remaining)
            .unwrap_or(policy.max_chunk_bytes)
            .min(policy.max_chunk_bytes);
        let chunk = source.read_chunk(descriptor, offset, requested)?;
        if chunk.is_empty() {
            return Err(UpdateError::Transfer(
                "artifact source ended before the declared size".to_string(),
            ));
        }
        if chunk.len() > requested {
            return Err(UpdateError::Transfer(
                "artifact source returned more bytes than requested".to_string(),
            ));
        }
        let chunk_len = u64::try_from(chunk.len()).map_err(|_| {
            UpdateError::Transfer("artifact chunk length overflows offset".to_string())
        })?;
        let next_offset = offset
            .checked_add(chunk_len)
            .ok_or_else(|| UpdateError::Transfer("artifact offset overflows".to_string()))?;
        if next_offset > descriptor.size_bytes() {
            return Err(UpdateError::Transfer(
                "artifact source exceeded the declared size".to_string(),
            ));
        }
        writer.write_chunk(offset, &chunk)?;
        hasher.update(&chunk);
        offset = next_offset;
    }
    if encode_hex(&hasher.finalize()) != descriptor.sha256() {
        return Err(UpdateError::ChecksumMismatch);
    }
    Ok(())
}

/// Local file source for bounded artifact transfer.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileArtifactSource;

impl ArtifactSource for FileArtifactSource {
    fn read_chunk(
        &self,
        descriptor: &ArtifactDescriptor,
        offset: u64,
        max_len: usize,
    ) -> UpdateResult<Vec<u8>> {
        if max_len == 0 || max_len > DEFAULT_ARTIFACT_CHUNK_BYTES {
            return Err(UpdateError::Transfer(
                "requested chunk exceeds the local source limit".to_string(),
            ));
        }
        if offset >= descriptor.size_bytes() {
            return Err(UpdateError::Transfer(
                "artifact offset is outside the declared size".to_string(),
            ));
        }
        let path = descriptor
            .artifact_reference()
            .strip_prefix("file:")
            .ok_or_else(|| {
                UpdateError::Provider("file source requires a file: reference".to_string())
            })?;
        let mut file = open_regular_file(Path::new(path))
            .map_err(|error| UpdateError::Provider(error.to_string()))?;
        let length = file
            .metadata()
            .map_err(|error| UpdateError::Provider(error.to_string()))?
            .len();
        if length != descriptor.size_bytes() {
            return Err(UpdateError::ChecksumMismatch);
        }
        file.seek(SeekFrom::Start(offset))
            .map_err(|error| UpdateError::Provider(error.to_string()))?;
        let remaining = descriptor.size_bytes() - offset;
        let capacity = usize::try_from(remaining).unwrap_or(max_len).min(max_len);
        let mut bytes = vec![0_u8; capacity];
        file.read_exact(&mut bytes)
            .map_err(|error| UpdateError::Provider(error.to_string()))?;
        Ok(bytes)
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
