// =============================================================================
//        #######
//     ###       ###     F: private_fixtures.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 10:22:10 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 1.0.3-rc.4
// =============================================================================

// appcore-norm: test
//! Cross-platform fixture contract; Windows must additionally run under limited/elevated tokens.
#![cfg(feature = "test-fixtures")]
use appcore_security::{
    test_fixtures::{random_fixture_secret, write_secret_fixture},
    FileSecretResolver, PrivateDirectoryGuard, SecretBytes, SecretResolver, SecuritySecretRef,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    path: PathBuf,
    guard: Option<PrivateDirectoryGuard>,
}
impl Fixture {
    fn new() -> Self {
        #[cfg(windows)]
        let temp = std::env::temp_dir();
        #[cfg(not(windows))]
        let temp = std::env::temp_dir().canonicalize().unwrap();
        let path = temp.join(format!(
            "security-fixture-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        #[cfg(windows)]
        let guard = appcore_security::create_update_private_directory(&path).unwrap();
        #[cfg(not(windows))]
        let guard = appcore_security::create_private_directory(&path).unwrap();
        Self {
            path,
            guard: Some(guard),
        }
    }
    fn guard(&self) -> &PrivateDirectoryGuard {
        self.guard.as_ref().unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.guard.take();
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
#[test]
fn random_private_fixture_is_accepted_by_production_reader_and_never_overwritten() {
    let f = Fixture::new();
    let secret = random_fixture_secret().unwrap();
    let other = random_fixture_secret().unwrap();
    assert_ne!(secret, other);
    assert_eq!(secret.as_bytes().len(), 32);
    assert_eq!(format!("{secret:?}"), "SecretBytes(REDACTED)");
    write_secret_fixture(f.guard(), "credential.bin", &secret).unwrap();
    let resolver = FileSecretResolver::new(&f.path);
    assert_eq!(
        resolver
            .resolve(&SecuritySecretRef("credential.bin".into()))
            .unwrap(),
        secret
    );
    assert!(write_secret_fixture(f.guard(), "credential.bin", &other).is_err());
    assert_eq!(
        resolver
            .resolve(&SecuritySecretRef("credential.bin".into()))
            .unwrap(),
        secret
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(f.path.join("credential.bin"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
#[test]
fn invalid_names_and_payload_bounds_leave_directory_empty() {
    let f = Fixture::new();
    let secret = random_fixture_secret().unwrap();
    for name in [
        "",
        ".",
        "..",
        "../outside",
        "nested/file",
        "C:stream",
        "bad\nname",
    ] {
        assert!(write_secret_fixture(f.guard(), name, &secret).is_err());
    }
    for bytes in [Vec::new(), vec![0; 65_537]] {
        assert!(write_secret_fixture(f.guard(), "bad", &SecretBytes::new(bytes)).is_err());
    }
    assert_eq!(std::fs::read_dir(&f.path).unwrap().count(), 0);
}
#[cfg(unix)]
#[test]
fn symlink_and_insecure_parent_are_rejected_without_repair() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let f = Fixture::new();
    let secret = random_fixture_secret().unwrap();
    write_secret_fixture(f.guard(), "original", &secret).unwrap();
    symlink(f.path.join("original"), f.path.join("link")).unwrap();
    assert!(write_secret_fixture(f.guard(), "link", &secret).is_err());
    std::fs::set_permissions(&f.path, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(write_secret_fixture(f.guard(), "new", &secret).is_err());
    assert_eq!(
        std::fs::metadata(&f.path).unwrap().permissions().mode() & 0o777,
        0o755
    );
    assert!(!f.path.join("new").exists());
}
