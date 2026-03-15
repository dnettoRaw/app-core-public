// =============================================================================
//        #######
//     ###       ###     F: update_service_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/24 11:51:10 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;
use appcore_contracts::{ProviderConfig, ProviderId};

const TEST_PUBLIC_KEY: &str = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";

fn update_config() -> ProviderConfig {
    ProviderConfig::new(ProviderId::new("file-update").unwrap())
}

fn signed_config() -> ProviderConfig {
    update_config()
        .with_setting("signing_key.release", TEST_PUBLIC_KEY)
        .unwrap()
        .with_setting("allowed_channels", "stable")
        .unwrap()
        .with_setting("allowed_origins", "file:")
        .unwrap()
}

#[test]
fn empty_update_authenticity_policy_fails_bootstrap_validation() {
    assert!(validate_update_authenticity_config(&update_config()).is_err());
}

#[test]
fn removed_trusted_local_setting_hits_update_wall() {
    let config = update_config()
        .with_setting("trusted_local", "true")
        .unwrap();
    let error = validate_update_authenticity_config(&config)
        .expect_err("removed bypass must fail closed")
        .to_string();

    assert_eq!(error, "NO MORE SUPPORTED PLEASE UPDATE");
}

#[test]
fn complete_signed_policy_passes_bootstrap_validation() {
    assert!(validate_update_authenticity_config(&signed_config()).is_ok());
}

#[cfg(unix)]
#[test]
fn smoke_test_requires_successful_bounded_process() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!(
        "appcore-update-smoke-{}-{}",
        std::process::id(),
        crate::bootstrap::now_ms()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let success = root.join("success.sh");
    let failure = root.join("failure.sh");
    let timeout = root.join("timeout.sh");
    std::fs::write(&success, b"#!/bin/sh\nexit 0\n").unwrap();
    std::fs::write(&failure, b"#!/bin/sh\nexit 1\n").unwrap();
    std::fs::write(&timeout, b"#!/bin/sh\nsleep 2\n").unwrap();
    for path in [&success, &failure, &timeout] {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    assert!(run_smoke_test(&success, Duration::from_secs(1)).is_ok());
    assert!(run_smoke_test(&failure, Duration::from_secs(1)).is_err());
    assert!(run_smoke_test(&timeout, Duration::from_millis(100)).is_err());
    std::fs::remove_dir_all(root).unwrap();
}
