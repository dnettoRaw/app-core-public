// =============================================================================
//        #######
//     ###       ###     F: actions.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::{
    run_security_keyring_init, run_security_keyring_recover, run_security_keyring_revoke,
    run_security_keyring_rotate, run_security_keyring_status, run_security_secret_rotate,
    run_security_secret_status, run_supervisor, run_token_command, run_token_query, run_token_sync,
    BootstrapError, CliArgs, RuntimeCliCommand, SupervisorRunOptions,
};

pub(super) fn run_token_action(
    command: RuntimeCliCommand,
    parsed: &CliArgs,
) -> Result<(), BootstrapError> {
    match command {
        RuntimeCliCommand::TokenCommand => run_token_command(
            parsed.config_path.as_deref(),
            parsed.token_command.as_deref(),
            parsed.token_scope.as_deref(),
            parsed.token_subject.as_deref(),
            parsed.token_ttl_ms,
        ),
        RuntimeCliCommand::TokenSync => run_token_sync(
            parsed.config_path.as_deref(),
            parsed.token_subject.as_deref(),
            parsed.token_ttl_ms,
        ),
        RuntimeCliCommand::TokenQuery => run_token_query(
            parsed.config_path.as_deref(),
            parsed.token_query.as_deref(),
            parsed.token_scope.as_deref(),
            parsed.token_subject.as_deref(),
            parsed.token_ttl_ms,
        ),
        _ => Err(BootstrapError::Cli("invalid token command".to_string())),
    }
}

pub(super) fn run_supervisor_action(parsed: &CliArgs) -> Result<(), BootstrapError> {
    run_supervisor(SupervisorRunOptions {
        config_path: parsed.config_path.as_deref(),
        max_restarts: parsed.max_restarts,
        child_args: parsed.child_args.as_deref(),
        health_url: parsed.health_url.as_deref(),
        health_check_every_ticks: parsed.health_check_every_ticks,
        health_fail_limit: parsed.health_fail_limit,
        only_one: parsed.only_one,
        kill_others: parsed.kill_others,
    })
}

pub(super) fn run_security_action(parsed: &CliArgs) -> Result<(), BootstrapError> {
    if parsed.security_action.as_deref() != Some("secret") {
        return Err(BootstrapError::Cli("unknown security action".to_string()));
    }
    match parsed.security_secret_action.as_deref() {
        Some("status") => run_security_secret_status(parsed.config_path.as_deref()),
        Some("rotate") => {
            let out = parsed
                .security_out
                .as_deref()
                .ok_or_else(|| BootstrapError::Cli("missing --out".to_string()))?;
            run_security_secret_rotate(parsed.config_path.as_deref(), out)
        }
        Some("keyring-init") => {
            run_security_keyring_init(required_keyring(parsed)?, parsed.token_ttl_ms)
        }
        Some("keyring-rotate") => {
            run_security_keyring_rotate(required_keyring(parsed)?, parsed.token_ttl_ms)
        }
        Some("keyring-status") => run_security_keyring_status(required_keyring(parsed)?),
        Some("keyring-recover") => run_security_keyring_recover(required_keyring(parsed)?),
        Some("keyring-revoke") => run_security_keyring_revoke(
            required_keyring(parsed)?,
            parsed
                .security_key_id
                .as_deref()
                .ok_or_else(|| BootstrapError::Cli("missing --key-id".to_string()))?,
        ),
        _ => Err(BootstrapError::Cli(
            "unknown security secret action".to_string(),
        )),
    }
}

fn required_keyring(parsed: &CliArgs) -> Result<&str, BootstrapError> {
    parsed
        .security_keyring
        .as_deref()
        .ok_or_else(|| BootstrapError::Cli("missing --keyring".to_string()))
}
