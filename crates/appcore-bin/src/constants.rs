// =============================================================================
//        #######
//     ###       ###     F: constants.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/05 22:12:31 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Default values and host branding for appcore-bin.

pub const DEFAULT_APP_NAME: &str = "AppCore-Runtime";
pub const DEFAULT_APP_ID: &str = "minimal-app";
pub const RUNTIME_BIN: &str = "appcore-bin";
pub const DEFAULT_DEPLOYMENT_MANIFEST: &str = "deployment.toml";
pub const DEFAULT_AUTH_SERVER_APP_PASSWORD: &str = "";
pub const AUTH_SERVER_BIN: &str = "appcore-auth-server";
pub const EXPECTED_DATA_DIR_POLICY: &str = "platform-local-app-data";
pub const EXPECTED_CACHE_DIR_POLICY: &str = "platform-local-cache";
pub const SUPPORTED_COMMANDS: &[&str] = &[
    "server",
    "status",
    "health",
    "doctor",
    "config validate",
    "diagnostics",
    "export",
    "backup",
    "sync",
    "vault",
    "token command",
    "token sync",
    "token query",
    "idempotency compact",
    "supervisor",
    "security secret status",
    "security secret rotate",
    "security secret keyring-init",
    "security secret keyring-rotate",
    "security secret keyring-status",
    "security secret keyring-recover",
    "security secret keyring-revoke",
    "security auth-server install",
    "version",
    "build-info",
    "first-run",
    "run",
    "last-run",
    "paths",
    "completions",
];

pub const DEFAULT_HELP_LINES: &[&str] = &[
    "Core:",
    "  help | -h | --help          show this help",
    "  version | -V | --version    print app host version",
    "  build-info                  print embedded build metadata",
    "  paths                       print local app paths",
    "  first-run                   create local manifests, secret and .dnt markers",
    "  run [--watch]               run using the first-run deployment manifest",
    "  last-run [--dry-run]        show/remove local app data",
    "  last-run --purge            remove local app data and cache",
    "",
    "Runtime:",
    "  server --deployment <path>  start runtime host",
    "  status --deployment <path>  print runtime status",
    "  health --deployment <path>  run runtime health checks",
    "  doctor --deployment <path>  validate managed-service graph and policies",
    "  config validate --deployment <path>",
    "                              validate both versioned manifests",
    "  diagnostics --deployment <path>",
    "                              print redacted runtime diagnostics",
    "  export --out <path>         export diagnostics and in-memory audit",
    "  backup create --deployment <path> [--name <name>]",
    "  backup verify --deployment <path> --name <name>",
    "  backup restore --deployment <path> --name <name> --confirm-restore",
    "  backup drill --deployment <path> [--name <name>] --confirm-restore",
    "  sync [status|push]          inspect or push sync",
    "  supervisor --deployment <path>",
    "                              run watchdog supervisor",
    "",
    "Security:",
    "  token command|query|sync    generate signed bearer token",
    "  security secret status      inspect secret metadata",
    "  security secret rotate      create new secret file",
    "  security secret keyring-init    initialize an owner-only V1 keyring",
    "  security secret keyring-rotate  rotate the active key without downtime",
    "  security secret keyring-status  inspect active key metadata",
    "  security secret keyring-recover recover one unambiguous active pointer",
    "  security secret keyring-revoke  revoke one key by ID",
    "  first-run --auth-server-app <password>",
    "                              install optional auth-server companion",
    "",
    "Config overrides:",
    "  APPCORE_APP_NAME, APPCORE_APP_ID, APPCORE_DATA_DIR, APPCORE_CACHE_DIR",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeHostConstants {
    pub app_name: String,
    pub app_version: String,
    pub app_id: String,
    pub binary_name: String,
    pub auth_server_binary_name: String,
    pub default_deployment_manifest: String,
    pub supported_commands: Vec<String>,
    pub help_lines: Vec<String>,
}

impl RuntimeHostConstants {
    pub fn new(
        app_name: impl Into<String>,
        app_version: impl Into<String>,
    ) -> Result<Self, String> {
        let app_name = required_value("app_name", app_name.into())?;
        let app_version = required_value("app_version", app_version.into())?;
        Ok(Self {
            app_name,
            app_version,
            app_id: DEFAULT_APP_ID.to_string(),
            binary_name: RUNTIME_BIN.to_string(),
            auth_server_binary_name: AUTH_SERVER_BIN.to_string(),
            default_deployment_manifest: DEFAULT_DEPLOYMENT_MANIFEST.to_string(),
            supported_commands: strings(SUPPORTED_COMMANDS),
            help_lines: strings(DEFAULT_HELP_LINES),
        })
    }

    pub fn with_app_id(mut self, app_id: impl Into<String>) -> Result<Self, String> {
        self.app_id = required_value("app_id", app_id.into())?;
        Ok(self)
    }

    pub fn with_binary_name(mut self, binary_name: impl Into<String>) -> Result<Self, String> {
        self.binary_name = required_value("binary_name", binary_name.into())?;
        Ok(self)
    }

    pub fn with_auth_server_binary_name(
        mut self,
        binary_name: impl Into<String>,
    ) -> Result<Self, String> {
        self.auth_server_binary_name =
            required_value("auth_server_binary_name", binary_name.into())?;
        Ok(self)
    }

    pub fn with_supported_commands(mut self, commands: &[&str]) -> Self {
        self.supported_commands = strings(commands);
        self
    }

    pub fn with_help_lines(mut self, lines: &[&str]) -> Self {
        self.help_lines = strings(lines);
        self
    }
}

pub fn default_host_constants() -> RuntimeHostConstants {
    RuntimeHostConstants {
        app_name: nonempty_or(default_app_name(), DEFAULT_APP_NAME),
        app_version: nonempty_or(default_app_version(), env!("CARGO_PKG_VERSION")),
        app_id: DEFAULT_APP_ID.to_string(),
        binary_name: RUNTIME_BIN.to_string(),
        auth_server_binary_name: AUTH_SERVER_BIN.to_string(),
        default_deployment_manifest: DEFAULT_DEPLOYMENT_MANIFEST.to_string(),
        supported_commands: strings(SUPPORTED_COMMANDS),
        help_lines: strings(DEFAULT_HELP_LINES),
    }
}

fn nonempty_or(value: String, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value
    }
}

pub fn env_or_default(name: &str, fallback: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| fallback.to_string())
}

pub fn default_app_name() -> String {
    option_env!("APPCORE_BUILD_APP_NAME")
        .unwrap_or(DEFAULT_APP_NAME)
        .to_string()
}

pub fn default_app_version() -> String {
    option_env!("APPCORE_BUILD_APP_VERSION")
        .or(option_env!("APPCORE_BUILD_VERSION"))
        .unwrap_or(env!("CARGO_PKG_VERSION"))
        .to_string()
}

pub fn auth_server_app_password() -> &'static str {
    option_env!("APPCORE_BUILD_AUTH_SERVER_APP_PASSWORD")
        .unwrap_or(DEFAULT_AUTH_SERVER_APP_PASSWORD)
}

pub fn auth_server_app_gate_matches(password: &str) -> bool {
    !auth_server_app_password().is_empty() && password == auth_server_app_password()
}

pub fn auth_server_install_name() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "auth-server.mac"
    }
    #[cfg(target_os = "windows")]
    {
        "auth-server.windows.exe"
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        "auth-server.linux"
    }
}

fn required_value(name: &'static str, value: String) -> Result<String, String> {
    if value.trim().is_empty() {
        return Err(format!("{name} is required"));
    }
    Ok(value)
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

#[cfg(test)]
#[path = "constants_tests.rs"]
mod tests;
