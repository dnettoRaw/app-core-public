// =============================================================================
//        #######
//     ###       ###     F: cli_types.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/06 20:53:23 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! CLI command and parsed argument types.

use appcore_args::Shell;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeCliCommand {
    Help,
    Server,
    Status,
    Health,
    Doctor,
    ConfigValidate,
    Diagnostics,
    Export,
    Version,
    BuildInfo,
    FirstRun,
    Run,
    LastRun,
    Paths,
    UpdateRequired,
    Backup,
    Sync,
    Vault,
    TokenCommand,
    TokenSync,
    TokenQuery,
    IdempotencyCompact,
    Supervisor,
    Security,
    Completions,
    Complete,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CliArgs {
    pub command: Option<RuntimeCliCommand>,
    pub help_path: Vec<String>,
    pub config_path: Option<String>,
    pub backup_file: Option<String>,
    pub backup_action: Option<String>,
    pub backup_name: Option<String>,
    pub token_command: Option<String>,
    pub token_query: Option<String>,
    pub token_scope: Option<String>,
    pub token_subject: Option<String>,
    pub token_ttl_ms: Option<u64>,
    pub sync_action: Option<String>,
    pub max_restarts: Option<u64>,
    pub child_args: Option<String>,
    pub health_url: Option<String>,
    pub health_check_every_ticks: Option<u64>,
    pub health_fail_limit: Option<u64>,
    pub security_action: Option<String>,
    pub security_secret_action: Option<String>,
    pub security_out: Option<String>,
    pub security_keyring: Option<String>,
    pub security_keyring_provider: Option<String>,
    pub security_key_id: Option<String>,
    pub auth_server_app_password: Option<String>,
    pub completion_shell: Option<Shell>,
    pub completion_cursor_word: Option<usize>,
    pub completion_words: Vec<String>,
    pub status_json: bool,
    pub production: bool,
    pub confirm_restore: bool,
    pub dry_run: bool,
    pub purge: bool,
    pub watch: bool,
    pub only_one: Option<bool>,
    pub kill_others: Option<bool>,
    pub update_required: bool,
    pub unknown_command: Option<String>,
}
