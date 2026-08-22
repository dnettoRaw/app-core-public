// =============================================================================
//        #######
//     ###       ###     F: artifact.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{AiError, AiResult, ArtifactIdentity};
use sha2::{Digest, Sha256};
use std::fmt::{Display, Formatter};
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// SHA-256 content identity for one model artifact.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ArtifactDigest([u8; 32]);

impl ArtifactDigest {
    /// Calculates a digest over complete artifact bytes.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let digest: [u8; 32] = Sha256::digest(bytes).into();
        Self(digest)
    }

    /// Parses exactly 64 lowercase or uppercase hexadecimal digits.
    pub fn parse_hex(value: &str) -> AiResult<Self> {
        if value.len() != 64 {
            return Err(AiError::InvalidInput("artifact digest length"));
        }
        let mut digest = [0u8; 32];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            digest[index] = parse_pair(pair)?;
        }
        Ok(Self(digest))
    }

    /// Returns the raw digest bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Display for ArtifactDigest {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

fn parse_pair(pair: &[u8]) -> AiResult<u8> {
    let high = hex_value(pair[0]).ok_or(AiError::InvalidInput("artifact digest"))?;
    let low = hex_value(pair[1]).ok_or(AiError::InvalidInput("artifact digest"))?;
    Ok(high * 16 + low)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Bounded local cache that verifies bytes before atomic activation.
#[derive(Debug)]
pub struct LocalArtifactCache {
    root: PathBuf,
    max_artifact_bytes: u64,
    counter: AtomicU64,
}

impl LocalArtifactCache {
    /// Creates and canonicalizes an artifact root that must not be a symlink.
    pub fn new(root: impl AsRef<Path>, max_artifact_bytes: u64) -> AiResult<Self> {
        if max_artifact_bytes == 0 {
            return Err(AiError::InvalidInput("artifact cache size"));
        }
        fs::create_dir_all(root.as_ref()).map_err(|_| AiError::Capacity("artifact root"))?;
        let metadata =
            fs::symlink_metadata(root.as_ref()).map_err(|_| AiError::Capacity("artifact root"))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(AiError::Integrity("artifact root type"));
        }
        let root =
            fs::canonicalize(root.as_ref()).map_err(|_| AiError::Capacity("artifact root"))?;
        Ok(Self {
            root,
            max_artifact_bytes,
            counter: AtomicU64::new(0),
        })
    }

    /// Verifies and stores complete artifact bytes under their digest identity.
    pub fn store(&self, identity: &ArtifactIdentity, bytes: &[u8]) -> AiResult<PathBuf> {
        validate_bytes(identity, bytes, self.max_artifact_bytes)?;
        let final_path = self.path(identity.digest);
        match self.load(identity) {
            Ok(existing) if existing == bytes => return Ok(final_path),
            Ok(_) => return Err(AiError::Integrity("artifact cache collision")),
            Err(AiError::NotFound("artifact")) => {}
            Err(error) => return Err(error),
        }
        let sequence = self.counter.fetch_add(1, Ordering::Relaxed);
        let temporary = self.root.join(format!(
            ".{}.{}.{}.tmp",
            identity.digest,
            std::process::id(),
            sequence
        ));
        write_temporary(&temporary, bytes)?;
        match fs::hard_link(&temporary, &final_path) {
            Ok(()) => {
                fs::remove_file(&temporary)
                    .map_err(|_| AiError::Capacity("artifact temporary cleanup"))?;
                sync_parent(&self.root)?;
                Ok(final_path)
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                let _ = fs::remove_file(&temporary);
                let existing = self.load(identity)?;
                if existing == bytes {
                    Ok(final_path)
                } else {
                    Err(AiError::Integrity("artifact cache collision"))
                }
            }
            Err(_) => {
                let _ = fs::remove_file(&temporary);
                Err(AiError::Capacity("artifact activation"))
            }
        }
    }

    /// Loads and re-verifies a cached artifact with a bounded read.
    pub fn load(&self, identity: &ArtifactIdentity) -> AiResult<Vec<u8>> {
        if identity.size_bytes > self.max_artifact_bytes {
            return Err(AiError::LimitExceeded {
                kind: crate::LimitKind::InputBytes,
                actual: identity.size_bytes,
                limit: self.max_artifact_bytes,
            });
        }
        let path = self.path(identity.digest);
        let file = open_verified_artifact(&path, identity.size_bytes)?;
        let mut bytes = Vec::with_capacity(
            usize::try_from(identity.size_bytes).map_err(|_| AiError::Capacity("artifact"))?,
        );
        file.take(self.max_artifact_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|_| AiError::Integrity("artifact read"))?;
        validate_bytes(identity, &bytes, self.max_artifact_bytes)?;
        Ok(bytes)
    }

    /// Checks for a regular digest path without following a filesystem link.
    pub fn contains(&self, identity: &ArtifactIdentity) -> AiResult<bool> {
        match open_verified_artifact(&self.path(identity.digest), identity.size_bytes) {
            Ok(_) => Ok(true),
            Err(AiError::NotFound("artifact")) => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// Removes a regular cached artifact while rejecting link targets.
    pub fn remove(&self, identity: &ArtifactIdentity) -> AiResult<bool> {
        if !self.contains(identity)? {
            return Ok(false);
        }
        fs::remove_file(self.path(identity.digest))
            .map_err(|_| AiError::Capacity("artifact eviction"))?;
        sync_parent(&self.root)?;
        Ok(true)
    }

    /// Reads one bounded byte range without allocating the complete artifact.
    ///
    /// The complete file was verified during activation. Consumers of a
    /// segmented bundle must additionally verify the returned segment digest.
    pub fn load_range(
        &self,
        identity: &ArtifactIdentity,
        offset: u64,
        length: u64,
        max_bytes: u64,
    ) -> AiResult<Vec<u8>> {
        let end = offset
            .checked_add(length)
            .ok_or(AiError::InvalidInput("artifact range overflow"))?;
        if length == 0 || length > max_bytes || end > identity.size_bytes {
            return Err(AiError::LimitExceeded {
                kind: crate::LimitKind::InputBytes,
                actual: length,
                limit: max_bytes.min(identity.size_bytes.saturating_sub(offset)),
            });
        }
        let path = self.path(identity.digest);
        let capacity =
            usize::try_from(length).map_err(|_| AiError::Capacity("artifact range allocation"))?;
        let mut file = open_verified_artifact(&path, identity.size_bytes)?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|_| AiError::Integrity("artifact range seek"))?;
        let mut bytes = vec![0u8; capacity];
        file.read_exact(&mut bytes)
            .map_err(|_| AiError::Integrity("artifact range read"))?;
        Ok(bytes)
    }

    /// Returns the digest-derived cache path without trusting remote filenames.
    #[must_use]
    pub fn path(&self, digest: ArtifactDigest) -> PathBuf {
        self.root.join(format!("{digest}.artifact"))
    }
}

fn validate_bytes(identity: &ArtifactIdentity, bytes: &[u8], maximum: u64) -> AiResult<()> {
    let size = u64::try_from(bytes.len()).map_err(|_| AiError::Capacity("artifact"))?;
    if size > maximum {
        return Err(AiError::LimitExceeded {
            kind: crate::LimitKind::InputBytes,
            actual: size,
            limit: maximum,
        });
    }
    if size != identity.size_bytes || ArtifactDigest::from_bytes(bytes) != identity.digest {
        return Err(AiError::Integrity("artifact digest"));
    }
    Ok(())
}

fn write_temporary(path: &Path, bytes: &[u8]) -> AiResult<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| AiError::Capacity("artifact temporary file"))?;
    if file.write_all(bytes).is_err() || file.sync_all().is_err() {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(AiError::Capacity("artifact write"));
    }
    Ok(())
}

fn open_verified_artifact(path: &Path, expected_bytes: u64) -> AiResult<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    let file = open_no_follow(&mut options, path).map_err(map_artifact_open_error)?;
    let path_metadata = fs::symlink_metadata(path).map_err(map_artifact_open_error)?;
    let file_metadata = file
        .metadata()
        .map_err(|_| AiError::Integrity("artifact file metadata"))?;
    if metadata_is_link(&path_metadata)
        || metadata_is_link(&file_metadata)
        || !file_metadata.is_file()
        || file_metadata.len() != expected_bytes
    {
        return Err(AiError::Integrity("artifact file metadata"));
    }
    Ok(file)
}

fn map_artifact_open_error(error: io::Error) -> AiError {
    if error.kind() == io::ErrorKind::NotFound {
        AiError::NotFound("artifact")
    } else {
        AiError::Integrity("artifact file open")
    }
}

#[cfg(unix)]
fn open_no_follow(options: &mut OpenOptions, path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    options.custom_flags(libc::O_NOFOLLOW).open(path)
}

#[cfg(windows)]
fn open_no_follow(options: &mut OpenOptions, path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
    options
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(all(not(unix), not(windows)))]
fn open_no_follow(_options: &mut OpenOptions, _path: &Path) -> io::Result<File> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "no-follow artifact opening is unavailable on this platform",
    ))
}

fn metadata_is_link(metadata: &Metadata) -> bool {
    metadata.file_type().is_symlink() || metadata_is_reparse_point(metadata)
}

#[cfg(windows)]
fn metadata_is_reparse_point(metadata: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn metadata_is_reparse_point(_metadata: &Metadata) -> bool {
    false
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> AiResult<()> {
    let mut options = OpenOptions::new();
    options.read(true);
    open_no_follow(&mut options, path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| AiError::Capacity("artifact directory sync"))
}

#[cfg(not(unix))]
fn sync_parent(_path: &Path) -> AiResult<()> {
    Ok(())
}
