// =============================================================================
//        #######
//     ###       ###     F: asset.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded asset contracts and behavior for this crate.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use sha2::{Digest, Sha256};

use crate::{ErrorCode, FileMakerError, FontAsset, FontResolver, Result};

/// Explicit resolved asset bytes and metadata.
#[derive(Clone, Debug)]
pub struct Asset {
    /// Stable logical name.
    pub name: String,
    /// Declared media type.
    pub media_type: String,
    /// Immutable bytes.
    pub bytes: Arc<[u8]>,
    /// SHA-256 digest.
    pub digest: [u8; 32],
}

impl Asset {
    /// Creates an asset and computes its digest.
    #[must_use]
    pub fn new(name: impl Into<String>, media_type: impl Into<String>, bytes: Vec<u8>) -> Self {
        let digest = Sha256::digest(&bytes).into();
        Self {
            name: name.into(),
            media_type: media_type.into(),
            bytes: bytes.into(),
            digest,
        }
    }
}

/// Read-only explicit asset resolver.
pub trait AssetResolver: Send + Sync {
    /// Resolves exact logical name with a caller-supplied byte cap.
    fn resolve_asset(&self, name: &str, max_bytes: usize) -> Result<Asset>;
}

/// Read-only explicit template include resolver.
pub trait TemplateResolver: Send + Sync {
    /// Resolves exact logical include path with a caller-supplied byte cap.
    fn resolve_template(&self, path: &str, max_bytes: usize) -> Result<Vec<u8>>;
}

#[derive(Clone, Debug)]
struct MemoryEntry {
    media_type: String,
    bytes: Arc<[u8]>,
    digest: [u8; 32],
}

/// In-memory resolver useful for embedded assets and deterministic tests.
#[derive(Clone, Debug, Default)]
pub struct MemoryResolver {
    entries: BTreeMap<String, MemoryEntry>,
}

impl MemoryResolver {
    /// Inserts or replaces one explicitly named entry.
    pub fn insert(
        &mut self,
        name: impl Into<String>,
        media_type: impl Into<String>,
        bytes: Vec<u8>,
    ) -> Result<()> {
        let name = name.into();
        validate_logical_path(&name)?;
        let digest = Sha256::digest(&bytes).into();
        self.entries.insert(
            name,
            MemoryEntry {
                media_type: media_type.into(),
                bytes: bytes.into(),
                digest,
            },
        );
        Ok(())
    }
}

impl AssetResolver for MemoryResolver {
    fn resolve_asset(&self, name: &str, max_bytes: usize) -> Result<Asset> {
        validate_logical_path(name)?;
        let entry = self
            .entries
            .get(name)
            .ok_or_else(|| asset_error("asset was not found"))?;
        if entry.bytes.len() > max_bytes {
            return Err(limit_error("asset bytes exceed configured limit"));
        }
        Ok(Asset {
            name: name.to_owned(),
            media_type: entry.media_type.clone(),
            bytes: Arc::clone(&entry.bytes),
            digest: entry.digest,
        })
    }
}

impl TemplateResolver for MemoryResolver {
    fn resolve_template(&self, path: &str, max_bytes: usize) -> Result<Vec<u8>> {
        validate_logical_path(path)?;
        let entry = self
            .entries
            .get(path)
            .ok_or_else(|| asset_error("template include was not found"))?;
        if entry.bytes.len() > max_bytes {
            return Err(limit_error("include bytes exceed configured limit"));
        }
        Ok(entry.bytes.to_vec())
    }
}

impl FontResolver for MemoryResolver {
    fn resolve_font(&self, name: &str, max_bytes: usize) -> Result<FontAsset> {
        validate_logical_path(name)?;
        let entry = self
            .entries
            .get(name)
            .ok_or_else(|| asset_error("font was not found"))?;
        if !matches!(entry.media_type.as_str(), "font/ttf" | "font/otf") {
            return Err(asset_error("font entry has an unsupported media type"));
        }
        if entry.bytes.len() > max_bytes {
            return Err(limit_error("font bytes exceed configured limit"));
        }
        FontAsset::new(name, entry.bytes.to_vec(), 0)
    }
}

/// Canonical-root filesystem resolver rejecting traversal and escaping symlinks.
#[derive(Clone, Debug)]
pub struct FileResolver {
    root: PathBuf,
}

impl FileResolver {
    /// Opens a canonical sandbox root.
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        let root = fs::canonicalize(root.as_ref())
            .map_err(|error| asset_error(format!("cannot open resolver root: {error}")))?;
        if !root.is_dir() {
            return Err(asset_error("resolver root is not a directory"));
        }
        Ok(Self { root })
    }

    fn load(&self, logical: &str, max_bytes: usize) -> Result<Vec<u8>> {
        validate_logical_path(logical)?;
        let candidate = fs::canonicalize(self.root.join(logical))
            .map_err(|error| asset_error(format!("cannot resolve `{logical}`: {error}")))?;
        if !candidate.starts_with(&self.root) {
            return Err(sandbox_error("resolved path escapes sandbox root"));
        }
        let mut options = OpenOptions::new();
        options.read(true);
        let mut file = open_no_follow(&mut options, &candidate)
            .map_err(|error| asset_error(format!("cannot open asset safely: {error}")))?;
        ensure_still_sandboxed(&self.root, &candidate)?;
        let metadata = file
            .metadata()
            .map_err(|error| asset_error(format!("cannot inspect asset: {error}")))?;
        let size = usize::try_from(metadata.len())
            .map_err(|_| limit_error("asset length exceeds platform range"))?;
        if !metadata.is_file() || metadata_is_link(&metadata) || size > max_bytes {
            return Err(limit_error("asset is not a bounded regular file"));
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(
                u64::try_from(max_bytes)
                    .unwrap_or(u64::MAX)
                    .saturating_add(1),
            )
            .read_to_end(&mut bytes)
            .map_err(|error| asset_error(format!("cannot read asset: {error}")))?;
        if bytes.len() > max_bytes {
            return Err(limit_error("asset changed beyond byte limit while reading"));
        }
        ensure_still_sandboxed(&self.root, &candidate)?;
        Ok(bytes)
    }
}

fn ensure_still_sandboxed(root: &Path, candidate: &Path) -> Result<()> {
    let current = fs::canonicalize(candidate)
        .map_err(|error| asset_error(format!("cannot revalidate asset path: {error}")))?;
    if current != candidate || !current.starts_with(root) {
        return Err(sandbox_error("asset path changed outside the sandbox"));
    }
    Ok(())
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
        "no-follow asset opening is unavailable on this platform",
    ))
}

#[cfg(windows)]
fn metadata_is_link(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn metadata_is_link(_metadata: &fs::Metadata) -> bool {
    false
}

impl AssetResolver for FileResolver {
    fn resolve_asset(&self, name: &str, max_bytes: usize) -> Result<Asset> {
        let bytes = self.load(name, max_bytes)?;
        Ok(Asset::new(name, media_type_for(name)?, bytes))
    }
}

impl TemplateResolver for FileResolver {
    fn resolve_template(&self, path: &str, max_bytes: usize) -> Result<Vec<u8>> {
        self.load(path, max_bytes)
    }
}

impl FontResolver for FileResolver {
    fn resolve_font(&self, name: &str, max_bytes: usize) -> Result<FontAsset> {
        if !matches!(media_type_for(name)?, "font/ttf" | "font/otf") {
            return Err(asset_error("font path must end in .ttf or .otf"));
        }
        FontAsset::new(name, self.load(name, max_bytes)?, 0)
    }
}

fn validate_logical_path(path: &str) -> Result<()> {
    let path = Path::new(path);
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(sandbox_error(
            "logical path must be non-empty, relative, and traversal-free",
        ));
    }
    Ok(())
}

fn media_type_for(name: &str) -> Result<&'static str> {
    match Path::new(name)
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => Ok("image/png"),
        Some("jpg" | "jpeg") => Ok("image/jpeg"),
        Some("svg") => Ok("image/svg+xml"),
        Some("yaml" | "yml") => Ok("application/yaml"),
        Some("ttf") => Ok("font/ttf"),
        Some("otf") => Ok("font/otf"),
        _ => Err(asset_error("asset extension has no declared media type")),
    }
}

fn sandbox_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::AssetSandbox, message)
}

fn asset_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::AssetInvalid, message)
}

fn limit_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::LimitExceeded, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_directory(name: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "appcore-filemaker-{name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&directory).unwrap();
        directory
    }

    #[test]
    fn rejects_traversal_before_resolution() {
        let mut resolver = MemoryResolver::default();
        assert_eq!(
            resolver
                .insert("../secret", "text/plain", Vec::new())
                .unwrap_err()
                .code(),
            ErrorCode::AssetSandbox
        );
    }

    #[test]
    fn memory_assets_share_immutable_bytes_between_resolutions() {
        let mut resolver = MemoryResolver::default();
        resolver
            .insert("images/shared.png", "image/png", vec![1, 2, 3, 4])
            .unwrap();
        let first = resolver.resolve_asset("images/shared.png", 4).unwrap();
        let second = resolver.resolve_asset("images/shared.png", 4).unwrap();
        assert!(Arc::ptr_eq(&first.bytes, &second.bytes));
        assert_eq!(first.digest, second.digest);
    }

    #[test]
    fn font_resolution_enforces_path_media_type_and_byte_cap() {
        let mut resolver = MemoryResolver::default();
        resolver
            .insert("fonts/body.ttf", "application/octet-stream", vec![0; 8])
            .unwrap();
        assert_eq!(
            resolver
                .resolve_font("fonts/body.ttf", 8)
                .unwrap_err()
                .code(),
            ErrorCode::AssetInvalid
        );

        resolver
            .insert("fonts/body.ttf", "font/ttf", vec![0; 8])
            .unwrap();
        assert_eq!(
            resolver
                .resolve_font("fonts/body.ttf", 7)
                .unwrap_err()
                .code(),
            ErrorCode::LimitExceeded
        );
        assert_eq!(
            resolver.resolve_font("../body.ttf", 8).unwrap_err().code(),
            ErrorCode::AssetSandbox
        );
    }

    #[test]
    fn filesystem_font_resolution_reuses_the_canonical_sandbox() {
        let resolver = FileResolver::new(".").unwrap();
        assert_eq!(
            resolver
                .resolve_font("../body.ttf", usize::MAX)
                .unwrap_err()
                .code(),
            ErrorCode::AssetSandbox
        );
    }

    #[test]
    fn filesystem_resolver_reads_only_bounded_regular_files() {
        let directory = temporary_directory("regular-file");
        fs::write(directory.join("image.png"), b"bounded").unwrap();
        let resolver = FileResolver::new(&directory).unwrap();
        assert_eq!(
            resolver
                .resolve_asset("image.png", 7)
                .unwrap()
                .bytes
                .as_ref(),
            b"bounded"
        );
        assert_eq!(
            resolver.resolve_asset("image.png", 6).unwrap_err().code(),
            ErrorCode::LimitExceeded
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn filesystem_resolver_rejects_a_symlink_escape() {
        use std::os::unix::fs::symlink;

        let root = temporary_directory("symlink-root");
        let outside = temporary_directory("symlink-outside");
        fs::write(outside.join("secret.png"), b"secret").unwrap();
        symlink(outside.join("secret.png"), root.join("escape.png")).unwrap();

        let resolver = FileResolver::new(&root).unwrap();
        assert_eq!(
            resolver.resolve_asset("escape.png", 64).unwrap_err().code(),
            ErrorCode::AssetSandbox
        );

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }
}
