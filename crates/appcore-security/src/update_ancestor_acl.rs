// =============================================================================
//        #######
//     ###       ###     F: update_ancestor_acl.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/07 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/07 by dnettoRaw
//      ###########      S: 1.0.3-rc.2
// =============================================================================
//! Validates Windows update-cache ancestors while retaining their directory handles.
//! Requesting FILE_LIST_DIRECTORY makes the no-delete sharing mode effective here.

use crate::{SecurityError, SecurityResult};
use std::ffi::c_void;
use std::fs::{File, OpenOptions};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::ptr;
use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, ERROR_SUCCESS, HANDLE};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, GetSecurityInfo, SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{
    EqualSid, GetAce, GetTokenInformation, TokenUser, ACCESS_ALLOWED_ACE,
    DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PSID, TOKEN_QUERY, TOKEN_USER,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_SHARE_READ, FILE_SHARE_WRITE,
};
use windows_sys::Win32::System::SystemServices::{
    ACCESS_ALLOWED_ACE_TYPE, ACCESS_ALLOWED_CALLBACK_ACE_TYPE,
    ACCESS_ALLOWED_CALLBACK_OBJECT_ACE_TYPE, ACCESS_ALLOWED_COMPOUND_ACE_TYPE,
    ACCESS_ALLOWED_OBJECT_ACE_TYPE,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

pub(crate) fn open_directory_no_delete(path: &Path) -> SecurityResult<File> {
    let file = OpenOptions::new()
        .access_mode(0x0002_0081) // READ_CONTROL | FILE_READ_ATTRIBUTES | FILE_LIST_DIRECTORY.
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| SecurityError::SecretUnavailable)?;
    let metadata = file
        .metadata()
        .map_err(|_| SecurityError::SecretUnavailable)?;
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(SecurityError::InvalidSecretRef);
    }
    Ok(file)
}

/// Accepts OS-owned ancestors, but never an untrusted owner or replacement grant.
pub(crate) fn open_trusted_update_ancestor(path: &Path) -> SecurityResult<File> {
    let file = open_directory_no_delete(path)?;
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
    if result != ERROR_SUCCESS || descriptor.is_null() {
        return Err(SecurityError::SecretUnavailable);
    }
    let _descriptor = LocalAllocation(descriptor);
    let token = CurrentUser::open()?;
    let user = token.sid()?;
    if owner.is_null() || dacl.is_null() || !trusted_sid(owner, user)? {
        return Err(SecurityError::InvalidSecretRef);
    }
    // DELETE, WRITE_DAC/OWNER, file writes, DELETE_CHILD, GENERIC_WRITE/ALL.
    // Creating an isolated child alone cannot replace an existing protected child.
    const MUTATION: u32 = 0x500d_0152;
    for index in 0..u32::from(unsafe { (*dacl).AceCount }) {
        let mut raw = ptr::null_mut();
        if unsafe { GetAce(dacl, index, &mut raw) } == 0 || raw.is_null() {
            return Err(SecurityError::SecretUnavailable);
        }
        let ace = raw.cast::<ACCESS_ALLOWED_ACE>();
        let header = unsafe { (*ace).Header };
        if header.AceFlags & 0x08 != 0 {
            continue; // INHERIT_ONLY_ACE does not grant rights on this ancestor.
        }
        if header.AceType == ACCESS_ALLOWED_ACE_TYPE as u8 {
            if usize::from(header.AceSize) < std::mem::size_of::<ACCESS_ALLOWED_ACE>() {
                return Err(SecurityError::InvalidSecretRef);
            }
            let sid = unsafe { ptr::addr_of_mut!((*ace).SidStart).cast() };
            if unsafe { (*ace).Mask } & MUTATION != 0 && !trusted_sid(sid, user)? {
                return Err(SecurityError::InvalidSecretRef);
            }
        } else if [
            ACCESS_ALLOWED_COMPOUND_ACE_TYPE,
            ACCESS_ALLOWED_OBJECT_ACE_TYPE,
            ACCESS_ALLOWED_CALLBACK_ACE_TYPE,
            ACCESS_ALLOWED_CALLBACK_OBJECT_ACE_TYPE,
        ]
        .contains(&u32::from(header.AceType))
        {
            // Conditional/object grants require a full evaluator; fail closed.
            return Err(SecurityError::InvalidSecretRef);
        }
    }
    Ok(file)
}

fn trusted_sid(sid: PSID, user: PSID) -> SecurityResult<bool> {
    if unsafe { EqualSid(sid, user) } != 0 {
        return Ok(true);
    }
    let mut text = ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 || text.is_null() {
        return Err(SecurityError::SecretUnavailable);
    }
    let _text = LocalAllocation(text.cast());
    let mut length = 0;
    while length < 256 && unsafe { *text.add(length) } != 0 {
        length += 1;
    }
    if length == 256 {
        return Err(SecurityError::InvalidSecretRef);
    }
    let value = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, length) });
    Ok(matches!(
        value.as_str(),
        "S-1-5-18" // SYSTEM.
            | "S-1-5-32-544" // Administrators.
            | "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464" // TrustedInstaller.
    ))
}

struct CurrentUser {
    handle: HANDLE,
    buffer: Vec<u8>,
}

impl CurrentUser {
    fn open() -> SecurityResult<Self> {
        let mut handle = ptr::null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut handle) } == 0 {
            return Err(SecurityError::SecretUnavailable);
        }
        let mut token = Self {
            handle,
            buffer: Vec::new(),
        };
        let mut required = 0;
        unsafe { GetTokenInformation(handle, TokenUser, ptr::null_mut(), 0, &mut required) };
        if required == 0 {
            return Err(SecurityError::SecretUnavailable);
        }
        token.buffer.resize(required as usize, 0);
        if unsafe {
            GetTokenInformation(
                handle,
                TokenUser,
                token.buffer.as_mut_ptr().cast(),
                required,
                &mut required,
            )
        } == 0
        {
            return Err(SecurityError::SecretUnavailable);
        }
        Ok(token)
    }

    fn sid(&self) -> SecurityResult<PSID> {
        if self.buffer.len() < std::mem::size_of::<TOKEN_USER>() {
            return Err(SecurityError::SecretUnavailable);
        }
        // The byte buffer need not satisfy TOKEN_USER alignment.
        let token = unsafe { self.buffer.as_ptr().cast::<TOKEN_USER>().read_unaligned() };
        Ok(token.User.Sid)
    }
}

impl Drop for CurrentUser {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.handle) };
    }
}

struct LocalAllocation(*mut c_void);

impl Drop for LocalAllocation {
    fn drop(&mut self) {
        unsafe { LocalFree(self.0) };
    }
}
