// =============================================================================
//        #######
//     ###       ###     F: secret_file.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 14:12:17 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Hardened deployment-local file secret resolution.

use crate::{SecretBytes, SecretResolver, SecurityError, SecurityResult, SecuritySecretRef};
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

const MAX_FILE_SECRET_BYTES: u64 = 65_536;

/// Resolves relative security references below one owner-only root.
#[derive(Debug, Clone)]
pub struct FileSecretResolver {
    root: PathBuf,
}

impl FileSecretResolver {
    /// Creates a resolver rooted at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl SecretResolver for FileSecretResolver {
    fn resolve(&self, reference: &SecuritySecretRef) -> SecurityResult<SecretBytes> {
        let relative = validate_relative_reference(&reference.0)?;
        validate_private_directory(&self.root)?;
        let canonical_root = fs::canonicalize(&self.root).map_err(unavailable)?;
        let path = self.root.join(relative);
        reject_symlink_components(&self.root, relative)?;
        let canonical_path = fs::canonicalize(&path).map_err(unavailable)?;
        if !canonical_path.starts_with(&canonical_root) {
            return Err(SecurityError::InvalidSecretRef);
        }
        let mut file = open_no_follow(&path)?;
        validate_private_file(&file)?;
        let length = file.metadata().map_err(unavailable)?.len();
        if length == 0 || length > MAX_FILE_SECRET_BYTES {
            return Err(SecurityError::SecretUnavailable);
        }
        let mut value = Vec::with_capacity(length as usize);
        file.read_to_end(&mut value).map_err(unavailable)?;
        Ok(SecretBytes::new(value))
    }
}

fn validate_relative_reference(value: &str) -> SecurityResult<&Path> {
    let path = Path::new(value);
    if value.is_empty()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(SecurityError::InvalidSecretRef);
    }
    Ok(path)
}

fn reject_symlink_components(root: &Path, relative: &Path) -> SecurityResult<()> {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        let metadata = fs::symlink_metadata(&current).map_err(unavailable)?;
        if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
            return Err(SecurityError::InvalidSecretRef);
        }
    }
    Ok(())
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
}

#[cfg(unix)]
fn open_no_follow(path: &Path) -> SecurityResult<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(unavailable)
}

#[cfg(windows)]
fn open_no_follow(path: &Path) -> SecurityResult<File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
    OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(unavailable)
}

#[cfg(all(not(unix), not(windows)))]
fn open_no_follow(_path: &Path) -> SecurityResult<File> {
    Err(SecurityError::SecretUnavailable)
}

#[cfg(unix)]
fn validate_private_directory(path: &Path) -> SecurityResult<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let metadata = fs::symlink_metadata(path).map_err(unavailable)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.uid() != unsafe { libc::geteuid() }
    {
        return Err(SecurityError::InvalidSecretRef);
    }
    Ok(())
}

#[cfg(windows)]
fn validate_private_directory(path: &Path) -> SecurityResult<()> {
    let metadata = fs::symlink_metadata(path).map_err(unavailable)?;
    if metadata.file_type().is_symlink() || is_reparse_point(&metadata) || !metadata.is_dir() {
        return Err(SecurityError::InvalidSecretRef);
    }
    windows_acl::validate_path_owner_acl(path)
}

#[cfg(all(not(unix), not(windows)))]
fn validate_private_directory(_path: &Path) -> SecurityResult<()> {
    Err(SecurityError::SecretUnavailable)
}

#[cfg(unix)]
fn validate_private_file(file: &File) -> SecurityResult<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let metadata = file.metadata().map_err(unavailable)?;
    if !metadata.is_file()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.uid() != unsafe { libc::geteuid() }
    {
        return Err(SecurityError::InvalidSecretRef);
    }
    Ok(())
}

#[cfg(windows)]
fn validate_private_file(file: &File) -> SecurityResult<()> {
    let metadata = file.metadata().map_err(unavailable)?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        return Err(SecurityError::InvalidSecretRef);
    }
    windows_acl::validate_file_owner_acl(file)
}

#[cfg(all(not(unix), not(windows)))]
fn validate_private_file(_file: &File) -> SecurityResult<()> {
    Err(SecurityError::SecretUnavailable)
}

fn unavailable<T>(_error: T) -> SecurityError {
    SecurityError::SecretUnavailable
}

#[cfg(windows)]
mod windows_acl {
    use super::*;
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::AsRawHandle;
    use std::ptr;
    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, ERROR_SUCCESS, HANDLE};
    use windows_sys::Win32::Security::Authorization::{
        GetNamedSecurityInfoW, GetSecurityInfo, SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::{
        EqualSid, GetAce, GetTokenInformation, TokenUser, ACCESS_ALLOWED_ACE, ACL,
        DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
        TOKEN_QUERY, TOKEN_USER,
    };
    use windows_sys::Win32::System::SystemServices::{
        ACCESS_ALLOWED_ACE_TYPE, ACCESS_ALLOWED_CALLBACK_ACE_TYPE,
        ACCESS_ALLOWED_CALLBACK_OBJECT_ACE_TYPE, ACCESS_ALLOWED_OBJECT_ACE_TYPE,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    pub(super) fn validate_path_owner_acl(path: &Path) -> SecurityResult<()> {
        let wide = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let mut owner = ptr::null_mut();
        let mut dacl = ptr::null_mut();
        let mut descriptor = ptr::null_mut();
        let result = unsafe {
            GetNamedSecurityInfoW(
                wide.as_ptr(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                &mut owner,
                ptr::null_mut(),
                &mut dacl,
                ptr::null_mut(),
                &mut descriptor,
            )
        };
        validate_security_result(result, descriptor, owner, dacl)
    }

    pub(super) fn validate_file_owner_acl(file: &File) -> SecurityResult<()> {
        let mut owner = ptr::null_mut();
        let mut dacl = ptr::null_mut();
        let mut descriptor = ptr::null_mut();
        let result = unsafe {
            GetSecurityInfo(
                file.as_raw_handle() as HANDLE,
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                &mut owner,
                ptr::null_mut(),
                &mut dacl,
                ptr::null_mut(),
                &mut descriptor,
            )
        };
        validate_security_result(result, descriptor, owner, dacl)
    }

    fn validate_security_result(
        result: u32,
        descriptor: PSECURITY_DESCRIPTOR,
        owner: PSID,
        dacl: *mut ACL,
    ) -> SecurityResult<()> {
        if result != ERROR_SUCCESS || descriptor.is_null() {
            return Err(SecurityError::SecretUnavailable);
        }
        let descriptor = SecurityDescriptor(descriptor);
        let validation = validate_owner_and_acl(owner, dacl);
        drop(descriptor);
        validation
    }

    fn validate_owner_and_acl(owner: PSID, dacl: *mut ACL) -> SecurityResult<()> {
        if owner.is_null() || dacl.is_null() {
            return Err(SecurityError::InvalidSecretRef);
        }
        let user = current_user()?;
        if unsafe { EqualSid(owner, user.sid()?) } == 0 {
            return Err(SecurityError::InvalidSecretRef);
        }
        validate_owner_only_acl(dacl, owner)
    }

    fn validate_owner_only_acl(dacl: *mut ACL, owner: PSID) -> SecurityResult<()> {
        let mut owner_allowed = false;
        let ace_count = unsafe { (*dacl).AceCount };
        for index in 0..u32::from(ace_count) {
            let mut raw_ace: *mut c_void = ptr::null_mut();
            if unsafe { GetAce(dacl, index, &mut raw_ace) } == 0 || raw_ace.is_null() {
                return Err(SecurityError::SecretUnavailable);
            }
            let ace_type = unsafe { (*(raw_ace.cast::<ACCESS_ALLOWED_ACE>())).Header.AceType };
            if ace_type == ACCESS_ALLOWED_ACE_TYPE as u8 {
                let ace = raw_ace.cast::<ACCESS_ALLOWED_ACE>();
                let sid = unsafe { ptr::addr_of_mut!((*ace).SidStart).cast::<c_void>() };
                if unsafe { EqualSid(owner, sid) } == 0 {
                    return Err(SecurityError::InvalidSecretRef);
                }
                owner_allowed = true;
            } else if is_other_allow_ace(ace_type) {
                return Err(SecurityError::InvalidSecretRef);
            }
        }
        if owner_allowed {
            Ok(())
        } else {
            Err(SecurityError::InvalidSecretRef)
        }
    }

    fn is_other_allow_ace(ace_type: u8) -> bool {
        [
            ACCESS_ALLOWED_OBJECT_ACE_TYPE,
            ACCESS_ALLOWED_CALLBACK_ACE_TYPE,
            ACCESS_ALLOWED_CALLBACK_OBJECT_ACE_TYPE,
        ]
        .contains(&u32::from(ace_type))
    }

    fn current_user() -> SecurityResult<TokenUser> {
        let mut token: HANDLE = ptr::null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return Err(SecurityError::SecretUnavailable);
        }
        let token = TokenHandle(token);
        let mut required = 0;
        unsafe {
            GetTokenInformation(token.0, TokenUser, ptr::null_mut(), 0, &mut required);
        }
        if required == 0 {
            return Err(SecurityError::SecretUnavailable);
        }
        let mut buffer = vec![0_u8; required as usize];
        if unsafe {
            GetTokenInformation(
                token.0,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                required,
                &mut required,
            )
        } == 0
        {
            return Err(SecurityError::SecretUnavailable);
        }
        Ok(TokenUser { buffer })
    }

    struct TokenUser {
        buffer: Vec<u8>,
    }

    impl TokenUser {
        fn sid(&self) -> SecurityResult<PSID> {
            if self.buffer.len() < std::mem::size_of::<TOKEN_USER>() {
                return Err(SecurityError::SecretUnavailable);
            }
            // SAFETY: GetTokenInformation initialized at least TOKEN_USER bytes.
            // The byte buffer has no TOKEN_USER alignment guarantee, so use an
            // unaligned value read instead of constructing a misaligned reference.
            let user = unsafe { self.buffer.as_ptr().cast::<TOKEN_USER>().read_unaligned() };
            Ok(user.User.Sid)
        }
    }

    struct TokenHandle(HANDLE);

    impl Drop for TokenHandle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    struct SecurityDescriptor(PSECURITY_DESCRIPTOR);

    impl Drop for SecurityDescriptor {
        fn drop(&mut self) {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
}
