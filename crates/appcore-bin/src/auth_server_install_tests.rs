// =============================================================================
//        #######
//     ###       ###     F: auth_server_install_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/06 22:13:35 by dnettoRaw
//    ##   ## ##   ##    U: 2026/06/07 09:02:12 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;
use crate::bootstrap::now_ms;
use crate::paths::{AppCorePaths, PathInputs};

fn temp_paths(name: &str) -> AppCorePaths {
    let root = std::env::temp_dir().join(format!("appcore-auth-{name}-{}", now_ms()));
    AppCorePaths::from_inputs(PathInputs {
        app_name: "DemoHost".to_string(),
        app_id: "demo".to_string(),
        home: Some(root.join("home")),
        data_root: Some(root.join("AppCore-Runtime")),
        cache_root: Some(root.join("AppCore-Runtime-cache")),
    })
    .expect("paths")
}

#[test]
fn auth_server_copy_requires_matching_gate() {
    let paths = temp_paths("bad-gate");
    let source = paths.data_dir.join("fake-auth-server");
    fs::create_dir_all(&paths.data_dir).expect("data dir");
    fs::write(&source, b"auth").expect("fake source");

    let install = install_auth_server_from_source(&paths, "wrong", &source);

    assert!(matches!(install, Err(BootstrapError::Cli(_))));
    assert!(!paths.auth_server_secret_file().exists());
    assert!(!paths.auth_transport_secret_file().exists());
    assert!(!paths.auth_required_file().exists());
    let _ = fs::remove_dir_all(paths.data_dir);
}

#[test]
fn auth_server_artifacts_can_be_prepared_without_printing_secret() {
    let paths = temp_paths("prepare-artifacts");
    let source = paths.data_dir.join("fake-auth-server");
    fs::create_dir_all(&paths.data_dir).expect("data dir");
    fs::write(&source, b"auth").expect("fake source");

    assert!(prepare_auth_server_dirs(&paths).is_ok());
    assert!(write_auth_server_secret_if_missing(&paths).is_ok());
    assert!(write_auth_transport_secret_if_missing(&paths).is_ok());
    assert!(write_auth_required_marker(&paths).is_ok());
    assert!(copy_auth_server_binary(&paths, &source).is_ok());

    assert!(paths.auth_server_file().exists());
    assert!(paths.auth_server_secret_file().exists());
    assert!(paths.auth_transport_secret_file().exists());
    assert!(paths.auth_required_file().exists());
    let marker = fs::read_to_string(paths.auth_required_file()).expect("marker");
    assert!(marker.contains("appcore.auth-required.v1"));
    assert!(marker.contains("auth-transport.secret"));
    let data_secret = fs::read(paths.auth_server_secret_file()).expect("data secret");
    let transport_secret = fs::read(paths.auth_transport_secret_file()).expect("transport secret");
    assert_ne!(data_secret, transport_secret);
    let _ = fs::remove_dir_all(paths.data_dir);
}
