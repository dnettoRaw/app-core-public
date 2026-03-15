// =============================================================================
//        #######
//     ###       ###     F: auth_server_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/06 22:13:35 by dnettoRaw
//    ##   ## ##   ##    U: 2026/06/07 12:31:50 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;
use appcore_security::{format_secret_material, new_rotated_secret};
use std::fs;

fn temp_secret_path(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("appcore-auth-cli-{name}-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("temp dir");
    let material = new_rotated_secret(None).expect("secret");
    let path = dir.join("auth.secret");
    fs::write(&path, format_secret_material(&material)).expect("secret file");
    path.to_string_lossy().to_string()
}

fn args(command: &str) -> Vec<String> {
    vec!["appcore-auth-server".to_string(), command.to_string()]
}

fn args_with(command: &str, flags: &[&str]) -> Vec<String> {
    let mut args = args(command);
    args.extend(flags.iter().map(|value| value.to_string()));
    args
}

#[test]
fn status_is_available_without_network() {
    assert!(run_auth_server_cli(&args("status")).is_ok());
}

#[test]
fn status_accepts_secret_path_without_printing_secret() {
    let args = args_with("status", &["--secret", "/tmp/appcore-auth.secret"]);

    assert!(run_auth_server_cli(&args).is_ok());
}

#[test]
fn grant_requires_explicit_secret_and_resource() {
    let result = run_auth_server_cli(&args("grant"));

    assert!(matches!(result, Err(error) if error.contains("--transport-secret")));
}

#[test]
fn grant_issues_short_lived_token_with_transport_secret_file() {
    let secret = temp_secret_path("grant");
    let args = args_with(
        "grant",
        &[
            "--transport-secret",
            &secret,
            "--resource",
            "storage/private.txt",
            "--ttl-ms",
            "1000",
        ],
    );

    let result = run_auth_server_cli(&args);

    assert!(result.is_ok());
    let _ = fs::remove_file(secret);
}

#[test]
fn status_accepts_transport_secret_path() {
    let secret = temp_secret_path("status-transport");
    let args = args_with("status", &["--transport-secret", &secret]);

    let result = run_auth_server_cli(&args);

    assert!(result.is_ok());
    let _ = fs::remove_file(secret);
}

#[test]
fn serve_requires_data_secret() {
    let result = run_auth_server_cli(&args("serve"));

    assert!(matches!(result, Err(error) if error.contains("--data-secret")));
}

#[test]
fn serve_requires_transport_secret() {
    let args = args_with(
        "serve",
        &[
            "--data-secret",
            "/tmp/appcore-auth.secret",
            "--bind",
            "127.0.0.1:39991",
            "--auto-restart",
        ],
    );

    let result = run_auth_server_cli(&args);

    assert!(matches!(result, Err(error) if error.contains("--transport-secret")));
}

#[test]
fn unknown_auth_server_flag_is_rejected() {
    let args = args_with("status", &["--bad"]);

    let result = run_auth_server_cli(&args);

    assert!(matches!(result, Err(error) if error.contains("unknown")));
}
