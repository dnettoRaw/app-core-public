// =============================================================================
//        #######
//     ###       ###     F: filesystem.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/24 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/24 00:00:00 by dnettoRaw
//      ###########      S: 1.0.3-rc
// =============================================================================

//! Owner-controlled directory creation and validation.

use crate::{SecurityError, SecurityResult};
use std::fs::{self, File, OpenOptions};
use std::path::{Component, Path, PathBuf};

/// A validated directory whose ancestor handles remain open for the lifetime
/// of the guard.
#[derive(Debug)]
pub struct PrivateDirectoryGuard {
    path: PathBuf,
    _pins: Vec<File>,
}

impl PrivateDirectoryGuard {
    /// Returns the validated directory path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Joins a path below the validated directory without allowing traversal.
    pub fn join(&self, relative: impl AsRef<Path>) -> SecurityResult<PathBuf> {
        let relative = relative.as_ref();
        validate_relative_path(relative)?;
        Ok(self.path.join(relative))
    }
}

/// Creates and validates an owner-controlled directory hierarchy.
pub fn create_private_directory(path: impl AsRef<Path>) -> SecurityResult<PrivateDirectoryGuard> {
    walk_private_directory(path.as_ref(), true)
}

fn walk_private_directory(
    path: &Path,
    create_missing: bool,
) -> SecurityResult<PrivateDirectoryGuard> {
    if path.as_os_str().is_empty() {
        return Err(SecurityError::InvalidSecretRef);
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        return Err(SecurityError::InvalidSecretRef);
    }
    let mut current = PathBuf::new();
    let mut pins = Vec::new();
    let components = path.components().collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        if matches!(component, Component::CurDir) {
            continue;
        }
        current.push(component);
        let created = match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if is_symlink_or_reparse(&metadata) || !metadata.is_dir() {
                    return Err(SecurityError::InvalidSecretRef);
                }
                false
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if !create_missing {
                    return Err(SecurityError::SecretUnavailable);
                }
                fs::create_dir(&current).map_err(|_| SecurityError::SecretUnavailable)?;
                true
            }
            Err(_) => return Err(SecurityError::SecretUnavailable),
        };
        if created {
            set_owner_only_permissions(&current)?;
        }
        if index + 1 == components.len() {
            validate_private_directory(&current)?;
        } else {
            validate_ancestor_directory(&current)?;
        }
        pins.push(open_directory_handle(&current)?);
    }
    Ok(PrivateDirectoryGuard {
        path: path.to_path_buf(),
        _pins: pins,
    })
}

/// Opens and validates an existing owner-controlled directory hierarchy.
pub fn open_private_directory(path: impl AsRef<Path>) -> SecurityResult<PrivateDirectoryGuard> {
    walk_private_directory(path.as_ref(), false)
}

fn validate_private_directory(path: &Path) -> SecurityResult<()> {
    let metadata = fs::symlink_metadata(path).map_err(|_| SecurityError::SecretUnavailable)?;
    if is_symlink_or_reparse(&metadata) || !metadata.is_dir() {
        return Err(SecurityError::InvalidSecretRef);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.uid() != unsafe { libc::geteuid() }
            || metadata.permissions().mode() & 0o077 != 0
        {
            return Err(SecurityError::InvalidSecretRef);
        }
    }
    #[cfg(windows)]
    crate::secret_file::windows_acl::validate_path_owner_acl(path)?;
    #[cfg(all(not(unix), not(windows)))]
    return Err(SecurityError::Unsupported("private directory validation"));
    Ok(())
}

fn validate_ancestor_directory(path: &Path) -> SecurityResult<()> {
    let metadata = fs::symlink_metadata(path).map_err(|_| SecurityError::SecretUnavailable)?;
    if is_symlink_or_reparse(&metadata) || !metadata.is_dir() {
        return Err(SecurityError::InvalidSecretRef);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let mode = metadata.permissions().mode();
        let writable_by_other = mode & 0o022 != 0;
        let safe_sticky_root = mode & 0o1000 != 0 && metadata.uid() == 0;
        if writable_by_other && !safe_sticky_root {
            return Err(SecurityError::InvalidSecretRef);
        }
    }
    #[cfg(windows)]
    crate::secret_file::windows_acl::validate_path_owner_acl(path)?;
    #[cfg(all(not(unix), not(windows)))]
    return Err(SecurityError::Unsupported("private ancestor validation"));
    Ok(())
}

fn set_owner_only_permissions(path: &Path) -> SecurityResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| SecurityError::SecretUnavailable)?;
    }
    #[cfg(windows)]
    crate::secret_file::windows_acl::set_path_owner_acl(path, true)?;
    #[cfg(all(not(unix), not(windows)))]
    return Err(SecurityError::Unsupported("private directory creation"));
    Ok(())
}

fn open_directory_handle(path: &Path) -> SecurityResult<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(path)
            .map_err(|_| SecurityError::SecretUnavailable)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        };
        return OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .map_err(|_| SecurityError::SecretUnavailable);
    }
    #[cfg(all(not(unix), not(windows)))]
    {
        let _ = path;
        Err(SecurityError::Unsupported("private directory handles"))
    }
}

fn is_symlink_or_reparse(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        return metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    }
    #[cfg(not(windows))]
    false
}

fn validate_relative_path(path: &Path) -> SecurityResult<()> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::Prefix(_)
                    | Component::RootDir
                    | Component::ParentDir
                    | Component::CurDir
            )
        })
        || path.components().any(|component| {
            matches!(component, Component::Normal(value) if value.to_string_lossy().chars().any(char::is_control))
        })
    {
        return Err(SecurityError::InvalidSecretRef);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn creates_owner_only_directory_and_pins_ancestors() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "appcore-security-private-directory-{}",
                std::process::id()
            ));
        let _ = fs::remove_dir_all(&root);
        let guard = create_private_directory(root.join("nested")).unwrap();
        assert_eq!(guard.path().file_name().unwrap(), "nested");
        assert_eq!(
            fs::metadata(guard.path()).unwrap().permissions().mode() & 0o777,
            0o700
        );
        drop(guard);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_group_writable_existing_ancestor() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "appcore-security-insecure-ancestor-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o770)).unwrap();
        assert!(create_private_directory(root.join("nested")).is_err());
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_traversal_and_unsafe_child_paths() {
        let root = std::fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "appcore-security-private-directory-boundary-{}",
                std::process::id()
            ));
        let _ = fs::remove_dir_all(&root);
        let guard = create_private_directory(&root).unwrap();
        assert!(guard.join("../outside").is_err());
        assert!(guard.join("./child").is_err());
        assert_eq!(guard.join("nested/file").unwrap(), root.join("nested/file"));
        assert!(create_private_directory(root.join("../escape")).is_err());
        drop(guard);
        fs::remove_dir_all(root).unwrap();
    }
}
