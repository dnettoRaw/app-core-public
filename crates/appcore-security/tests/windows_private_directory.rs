// =============================================================================
//        #######
//     ###       ###     F: windows_private_directory.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/12 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/12 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Native limited/elevated-token regression fixture for update-cache guards.
#![cfg(windows)]
use appcore_security::create_update_private_directory;
use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
fn path(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "appcore-private-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
#[test]
fn created_leaf_is_private_and_retained_guard_excludes_rename() {
    let root = path("pin");
    let replacement = path("replacement");
    let guard = create_update_private_directory(&root).unwrap();
    assert!(fs::rename(&root, &replacement).is_err());
    assert!(create_update_private_directory(&root).is_ok());
    drop(guard);
    fs::rename(&root, &replacement).unwrap();
    fs::remove_dir(&replacement).unwrap();
}
#[test]
fn insecure_existing_leaf_is_rejected_and_never_repaired() {
    let root = path("unsafe");
    drop(create_update_private_directory(&root).unwrap());
    assert!(Command::new("icacls")
        .arg(&root)
        .args(["/grant", "*S-1-1-0:(OI)(CI)F"])
        .status()
        .unwrap()
        .success());
    assert!(create_update_private_directory(&root).is_err());
    fs::remove_dir(&root).unwrap();
}
