// =============================================================================
//        #######
//     ###       ###     F: secret_dpapi.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.5.0-alpha.1
// =============================================================================

//! Non-interactive current-user DPAPI boundary.

use super::{SecretAccessError, SecretAccessResult, WINDOWS_DPAPI_USER_SECRET_KEYRING_FORMAT};
use std::ptr;
use zeroize::Zeroize;

use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};

pub(super) fn protect(plaintext: &[u8]) -> SecretAccessResult<Vec<u8>> {
    let input = blob(plaintext)?;
    let entropy = entropy_blob()?;
    let mut output = CRYPT_INTEGER_BLOB::default();
    let succeeded = unsafe {
        CryptProtectData(
            &input,
            ptr::null(),
            &entropy,
            ptr::null(),
            ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if succeeded == 0 {
        return Err(SecretAccessError::Io);
    }
    LocalBlob::new(output, false)?.copy()
}

pub(super) fn unprotect(ciphertext: &[u8]) -> SecretAccessResult<Vec<u8>> {
    let input = blob(ciphertext)?;
    let entropy = entropy_blob()?;
    let mut output = CRYPT_INTEGER_BLOB::default();
    let succeeded = unsafe {
        CryptUnprotectData(
            &input,
            ptr::null_mut(),
            &entropy,
            ptr::null(),
            ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if succeeded == 0 {
        return Err(SecretAccessError::InvalidMaterial);
    }
    LocalBlob::new(output, true)?.copy()
}

fn entropy_blob() -> SecretAccessResult<CRYPT_INTEGER_BLOB> {
    blob(WINDOWS_DPAPI_USER_SECRET_KEYRING_FORMAT.as_bytes())
}

fn blob(bytes: &[u8]) -> SecretAccessResult<CRYPT_INTEGER_BLOB> {
    let length = u32::try_from(bytes.len()).map_err(|_| SecretAccessError::InvalidMaterial)?;
    Ok(CRYPT_INTEGER_BLOB {
        cbData: length,
        pbData: bytes.as_ptr().cast_mut(),
    })
}

struct LocalBlob {
    value: CRYPT_INTEGER_BLOB,
    sensitive: bool,
}

impl LocalBlob {
    fn new(value: CRYPT_INTEGER_BLOB, sensitive: bool) -> SecretAccessResult<Self> {
        if value.pbData.is_null() || value.cbData == 0 {
            if !value.pbData.is_null() {
                unsafe {
                    LocalFree(value.pbData.cast());
                }
            }
            return Err(SecretAccessError::InvalidMaterial);
        }
        Ok(Self { value, sensitive })
    }

    fn copy(&self) -> SecretAccessResult<Vec<u8>> {
        let length =
            usize::try_from(self.value.cbData).map_err(|_| SecretAccessError::InvalidMaterial)?;
        Ok(unsafe { std::slice::from_raw_parts(self.value.pbData, length) }.to_vec())
    }
}

impl Drop for LocalBlob {
    fn drop(&mut self) {
        if self.value.pbData.is_null() {
            return;
        }
        if self.sensitive {
            let bytes = unsafe {
                std::slice::from_raw_parts_mut(self.value.pbData, self.value.cbData as usize)
            };
            bytes.zeroize();
        }
        unsafe {
            LocalFree(self.value.pbData.cast());
        }
    }
}
