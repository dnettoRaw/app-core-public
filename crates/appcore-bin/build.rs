// =============================================================================
//        #######
//     ###       ###     F: build.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/05 22:12:31 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    track_build_inputs();
    set_env("APPCORE_BUILD_ID", build_id());
    set_env(
        "APPCORE_BUILD_VERSION",
        env!("CARGO_PKG_VERSION").to_string(),
    );
    set_env("APPCORE_BUILD_DATE", utc_now());
    set_env("APPCORE_GIT_COMMIT", git_commit());
    set_env("APPCORE_TARGET", env_value("TARGET", "unknown-target"));
    set_env("APPCORE_PROFILE", env_value("PROFILE", "unknown-profile"));
    set_env(
        "APPCORE_BUILD_APP_NAME",
        env_value("APPCORE_BUILD_APP_NAME", "AppCore-Runtime"),
    );
    set_env(
        "APPCORE_BUILD_BINARY_NAME",
        env_value("APPCORE_BUILD_BINARY_NAME", "appcore-bin"),
    );
    set_optional_env("APPCORE_BUILD_AUTH_SERVER_APP_PASSWORD");
}

fn track_build_inputs() {
    for name in [
        "APPCORE_BUILD_ID",
        "APPCORE_BUILD_VERSION",
        "APPCORE_BUILD_DATE",
        "APPCORE_GIT_COMMIT",
        "APPCORE_TARGET",
        "APPCORE_PROFILE",
        "APPCORE_BUILD_APP_NAME",
        "APPCORE_BUILD_BINARY_NAME",
        "APPCORE_BUILD_AUTH_SERVER_APP_PASSWORD",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }
}

fn set_env(name: &str, value: String) {
    println!("cargo:rustc-env={name}={value}");
}

fn set_optional_env(name: &str) {
    if let Ok(value) = std::env::var(name) {
        set_env(name, value);
    }
}

fn build_id() -> String {
    std::env::var("APPCORE_BUILD_ID").unwrap_or_else(|_| {
        format!(
            "{}-dev-{}",
            env!("CARGO_PKG_VERSION"),
            short_git_commit().unwrap_or_else(|| "nogit".to_string())
        )
    })
}

fn utc_now() -> String {
    std::env::var("APPCORE_BUILD_DATE").unwrap_or_else(|_| {
        command_output("date", &["-u", "+%Y-%m-%dT%H:%M:%SZ"]).unwrap_or_else(|| {
            let secs = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_secs())
                .unwrap_or(0);
            format!("{secs}")
        })
    })
}

fn git_commit() -> String {
    std::env::var("APPCORE_GIT_COMMIT")
        .ok()
        .or_else(|| command_output("git", &["rev-parse", "HEAD"]))
        .unwrap_or_else(|| "unknown".to_string())
}

fn short_git_commit() -> Option<String> {
    command_output("git", &["rev-parse", "--short", "HEAD"])
}

fn env_value(name: &str, fallback: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| fallback.to_string())
}

fn command_output(command: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(command).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    Some(text.trim().to_string())
}
