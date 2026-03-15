// =============================================================================
//        #######
//     ###       ###     F: build_info.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/05 22:12:31 by dnettoRaw
//    ##   ## ##   ##    U: 2026/06/06 20:47:13 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Build metadata exposed by appcore-bin commands.

use crate::constants::RUNTIME_BIN;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildInfo {
    pub app_name: String,
    pub app_version: String,
    pub binary_name: String,
    pub version: &'static str,
    pub build_id: &'static str,
    pub built_at: &'static str,
    pub git_commit: &'static str,
    pub target: &'static str,
    pub profile: &'static str,
}

pub fn current_build_info() -> BuildInfo {
    BuildInfo {
        app_name: option_env!("APPCORE_BUILD_APP_NAME")
            .unwrap_or("AppCore-Runtime")
            .to_string(),
        app_version: option_env!("APPCORE_BUILD_APP_VERSION")
            .or(option_env!("APPCORE_BUILD_VERSION"))
            .unwrap_or(env!("CARGO_PKG_VERSION"))
            .to_string(),
        binary_name: option_env!("APPCORE_BUILD_BINARY_NAME")
            .unwrap_or(RUNTIME_BIN)
            .to_string(),
        version: option_env!("APPCORE_BUILD_VERSION").unwrap_or(env!("CARGO_PKG_VERSION")),
        build_id: option_env!("APPCORE_BUILD_ID").unwrap_or("dev"),
        built_at: option_env!("APPCORE_BUILD_DATE").unwrap_or("unknown"),
        git_commit: option_env!("APPCORE_GIT_COMMIT").unwrap_or("unknown"),
        target: option_env!("APPCORE_TARGET").unwrap_or("unknown"),
        profile: option_env!("APPCORE_PROFILE").unwrap_or("unknown"),
    }
}

pub fn print_version() {
    let info = current_build_info();
    println!("{}", info.version);
}

pub fn print_build_info() {
    let info = current_build_info();
    println!("app_name: {}", info.app_name);
    println!("app_version: {}", info.app_version);
    println!("binary_name: {}", info.binary_name);
    println!("version: {}", info.version);
    println!("build_id: {}", info.build_id);
    println!("built_at: {}", info.built_at);
    println!("git_commit: {}", info.git_commit);
    println!("target: {}", info.target);
    println!("profile: {}", info.profile);
}
