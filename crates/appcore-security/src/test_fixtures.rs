// =============================================================================
//        #######
//     ###       ###     F: test_fixtures.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 10:22:10 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 1.0.3-rc.4
// =============================================================================

//! Explicit opt-in test support using the production owner-only validators.
//! These helpers create new fixture files only; they never repair/replace an existing path.
use crate::{PrivateDirectoryGuard, SecretBytes, SecurityError as E, SecurityResult};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::Path,
};

/// Generates 32 unpredictable bytes with the OS RNG; memory is zeroized by SecretBytes.
pub fn random_fixture_secret() -> SecurityResult<SecretBytes> {
    let mut bytes = zeroize::Zeroizing::new(vec![0; 32]);
    getrandom::fill(&mut bytes).map_err(|_| E::SecretUnavailable)?;
    Ok(SecretBytes::new(bytes.to_vec()))
}

/// Creates one flat, owner-only credential fixture inside an already private directory.
/// Input is 1..=65536 bytes. The guard must remain alive while the fixture is used.
/// Windows uses the production current-token SID and protected DACL, without PowerShell.
/// Unix creates mode 0600 before writing. Existing files/symlinks are never overwritten.
/// No payload or OS error text is included in returned errors. This is test support,
/// not an encrypted secret store or a replacement for deployment credential provisioning.
pub fn write_secret_fixture(
    directory: &PrivateDirectoryGuard,
    name: &str,
    contents: &SecretBytes,
) -> SecurityResult<()> {
    if name.is_empty()
        || name.len() > 128
        || name == "."
        || name == ".."
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
        || contents.as_bytes().is_empty()
        || contents.as_bytes().len() > 65_536
    {
        return Err(E::InvalidSecretRef);
    }
    crate::secret_file::validate_private_directory(directory.path())?;
    let path = directory.join(name)?;
    let mut options = OpenOptions::new();
    options.create_new(true).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(&path).map_err(|_| E::SecretUnavailable)?;
    let result = write_new(&path, &mut file, contents);
    drop(file);
    if result.is_err() {
        let _ = std::fs::remove_file(&path);
    }
    result
}
fn write_new(path: &Path, file: &mut File, contents: &SecretBytes) -> SecurityResult<()> {
    #[cfg(windows)]
    crate::secret_file::windows_acl::set_path_owner_acl(path, false)?;
    #[cfg(not(windows))]
    let _ = path;
    crate::secret_file::validate_private_file(file)?;
    file.write_all(contents.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|_| E::SecretUnavailable)
}
