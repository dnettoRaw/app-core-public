// =============================================================================
//        #######
//     ###       ###     F: filesystem_windows.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/06 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/06 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Windows current-owner ACL validation for private transfer spool paths.

use crate::TransferError;
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::AsRawHandle;
use std::ptr;
use std::{fs::File, path::Path};
use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, ERROR_SUCCESS, HANDLE};
use windows_sys::Win32::Security::Authorization::{
    GetNamedSecurityInfoW, GetSecurityInfo, SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{
    EqualSid, GetAce, GetTokenInformation, TokenUser, ACCESS_ALLOWED_ACE, ACL,
    DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, TOKEN_QUERY,
    TOKEN_USER,
};
use windows_sys::Win32::System::SystemServices::{
    ACCESS_ALLOWED_ACE_TYPE, ACCESS_ALLOWED_CALLBACK_ACE_TYPE,
    ACCESS_ALLOWED_CALLBACK_OBJECT_ACE_TYPE, ACCESS_ALLOWED_OBJECT_ACE_TYPE,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

pub(crate) fn validate_path_owner_acl(path: &Path) -> Result<(), TransferError> {
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

pub(crate) fn validate_file_owner_acl(file: &File) -> Result<(), TransferError> {
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
) -> Result<(), TransferError> {
    if result != ERROR_SUCCESS || descriptor.is_null() {
        return Err(TransferError::Storage);
    }
    let descriptor = SecurityDescriptor(descriptor);
    let validation = validate_owner_and_acl(owner, dacl);
    drop(descriptor);
    validation
}

fn validate_owner_and_acl(owner: PSID, dacl: *mut ACL) -> Result<(), TransferError> {
    if owner.is_null() || dacl.is_null() {
        return Err(TransferError::UnsafePath);
    }
    let user = current_user()?;
    if unsafe { EqualSid(owner, user.sid()?) } == 0 {
        return Err(TransferError::UnsafePath);
    }
    validate_owner_only_acl(dacl, owner)
}

fn validate_owner_only_acl(owner_acl: *mut ACL, owner: PSID) -> Result<(), TransferError> {
    let mut owner_allowed = false;
    let ace_count = unsafe { (*owner_acl).AceCount };
    for index in 0..u32::from(ace_count) {
        let mut raw_ace: *mut c_void = ptr::null_mut();
        if unsafe { GetAce(owner_acl, index, &mut raw_ace) } == 0 || raw_ace.is_null() {
            return Err(TransferError::Storage);
        }
        let ace_type = unsafe { (*(raw_ace.cast::<ACCESS_ALLOWED_ACE>())).Header.AceType };
        if ace_type == ACCESS_ALLOWED_ACE_TYPE as u8 {
            let ace = raw_ace.cast::<ACCESS_ALLOWED_ACE>();
            let sid = unsafe { ptr::addr_of_mut!((*ace).SidStart).cast::<c_void>() };
            if unsafe { EqualSid(owner, sid) } == 0 {
                return Err(TransferError::UnsafePath);
            }
            owner_allowed = true;
        } else if is_other_allow_ace(ace_type) {
            return Err(TransferError::UnsafePath);
        }
    }
    if owner_allowed {
        Ok(())
    } else {
        Err(TransferError::UnsafePath)
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

fn current_user() -> Result<TokenUserBuffer, TransferError> {
    let mut token: HANDLE = ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(TransferError::Storage);
    }
    let token = TokenHandle(token);
    let mut required = 0;
    unsafe {
        GetTokenInformation(token.0, TokenUser, ptr::null_mut(), 0, &mut required);
    }
    if required == 0 {
        return Err(TransferError::Storage);
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
        return Err(TransferError::Storage);
    }
    Ok(TokenUserBuffer { buffer })
}

struct TokenUserBuffer {
    buffer: Vec<u8>,
}

impl TokenUserBuffer {
    fn sid(&self) -> Result<PSID, TransferError> {
        if self.buffer.len() < std::mem::size_of::<TOKEN_USER>() {
            return Err(TransferError::Storage);
        }
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
