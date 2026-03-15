// =============================================================================
//        #######
//     ###       ###     F: constants_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/06 20:47:13 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;

#[test]
fn host_constants_require_app_name_and_version() {
    assert!(RuntimeHostConstants::new("", "1.0.0").is_err());
    assert!(RuntimeHostConstants::new("Demo", "").is_err());
}

#[test]
fn host_constants_default_to_runtime_commands() {
    let constants = RuntimeHostConstants::new("Demo", "1.0.0").expect("constants");
    assert_eq!(constants.app_name, "Demo");
    assert_eq!(constants.app_version, "1.0.0");
    assert!(constants.supported_commands.contains(&"server".to_string()));
    assert!(constants
        .help_lines
        .iter()
        .any(|line| line.contains("Runtime")));
}

#[test]
fn host_constants_allow_command_and_help_override() {
    let constants = RuntimeHostConstants::new("Demo", "1.0.0")
        .expect("constants")
        .with_supported_commands(&["serve-demo"])
        .with_help_lines(&["serve-demo    start demo"]);
    assert_eq!(constants.supported_commands, vec!["serve-demo"]);
    assert_eq!(constants.help_lines, vec!["serve-demo    start demo"]);
}

#[test]
fn host_constants_allow_identity_overrides() {
    let constants = RuntimeHostConstants::new("Demo", "1.0.0")
        .expect("constants")
        .with_app_id("demo-app")
        .expect("app id")
        .with_binary_name("demo-bin")
        .expect("binary");
    assert_eq!(constants.app_id, "demo-app");
    assert_eq!(constants.binary_name, "demo-bin");
}

#[test]
fn auth_server_gate_is_disabled_without_build_password() {
    assert!(!auth_server_app_gate_matches(""));
    assert!(!auth_server_app_gate_matches("anything"));
}

#[test]
fn auth_server_install_name_has_platform_suffix() {
    assert!(auth_server_install_name().starts_with("auth-server."));
}
