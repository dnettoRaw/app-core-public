// =============================================================================
//        #######
//     ###       ###     F: cli.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/02 13:08:16 by dnettoRaw
//    ##   ## ##   ##    U: 2026/06/06 20:53:23 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Owns appcore-bin command-line parsing.

#[path = "cli_spec.rs"]
mod cli_spec;
#[path = "cli_types.rs"]
mod cli_types;

pub use cli_types::{CliArgs, RuntimeCliCommand};

use crate::constants::{default_host_constants, RuntimeHostConstants};
use appcore_args::{
    render_dynamic_completion_script, CliError, CliErrorKind, CliParser, CompletionEngine,
    CompletionRequest, HelpRenderer, ParsedCli, RawArgs, Shell,
};

const REMOVED_OPTIONS: &[&str] = &[
    "--config",
    "--payload",
    "--onlyone",
    "--no-onlyone",
    "--killothers",
    "--no-killothers",
];

/// Parses bounded arguments directly from the current process environment.
pub fn parse_cli_env() -> Result<CliArgs, CliError> {
    parse_raw_cli_args(&RawArgs::from_env()?)
}

/// Parses a compatibility argument vector that includes the executable name.
pub fn parse_cli_args(args: &[String]) -> CliArgs {
    let parsed =
        RawArgs::parse(args.iter().skip(1).cloned()).and_then(|raw| parse_raw_cli_args(&raw));
    match parsed {
        Ok(cli) => cli,
        Err(error) => CliArgs {
            unknown_command: Some(compatibility_error(args, &error)),
            ..CliArgs::default()
        },
    }
}

/// Parses already bounded arguments without an executable-name prefix.
pub fn parse_raw_cli_args(args: &RawArgs) -> Result<CliArgs, CliError> {
    if contains_removed_input(args.words()) {
        return Ok(CliArgs {
            update_required: true,
            ..CliArgs::default()
        });
    }
    let constants = default_host_constants();
    let spec = cli_spec::runtime_spec(
        &constants.binary_name,
        &format!("{} host", constants.app_name),
        &constants.app_version,
    );
    let parsed = CliParser::new(&spec).parse(args)?;
    parsed_cli(parsed)
}

pub(crate) fn render_help(
    constants: &RuntimeHostConstants,
    path: &[String],
) -> Result<String, String> {
    let spec = cli_spec::runtime_spec(
        &constants.binary_name,
        &format!("{} host", constants.app_name),
        &constants.app_version,
    );
    let path = path.iter().map(String::as_str).collect::<Vec<_>>();
    HelpRenderer::new(&spec)
        .render(&path)
        .map_err(|error| error.to_string())
}

pub(crate) fn completion_script(
    constants: &RuntimeHostConstants,
    shell: Shell,
) -> Result<String, String> {
    render_dynamic_completion_script(&constants.binary_name, &["complete"], shell)
        .map_err(|error| error.to_string())
}

pub(crate) fn completion_candidates(
    constants: &RuntimeHostConstants,
    cursor_word: usize,
    words: Vec<String>,
) -> Vec<String> {
    let spec = cli_spec::runtime_spec(
        &constants.binary_name,
        &format!("{} host", constants.app_name),
        &constants.app_version,
    );
    CompletionEngine::new(&spec)
        .complete(&CompletionRequest::new(words, cursor_word))
        .into_iter()
        .map(|candidate| candidate.value().to_string())
        .collect()
}

fn parsed_cli(parsed: ParsedCli) -> Result<CliArgs, CliError> {
    let mut cli = CliArgs {
        config_path: option(&parsed, "deployment"),
        backup_file: option(&parsed, "file"),
        backup_name: option(&parsed, "name"),
        token_command: option(&parsed, "command"),
        token_query: option(&parsed, "query"),
        token_scope: option(&parsed, "scope"),
        token_subject: option(&parsed, "subject"),
        token_ttl_ms: number(&parsed, "ttl-ms")?,
        max_restarts: number(&parsed, "max-restarts")?,
        child_args: option(&parsed, "child-args"),
        health_url: option(&parsed, "health-url"),
        health_check_every_ticks: number(&parsed, "health-check-every-ticks")?,
        health_fail_limit: number(&parsed, "health-fail-limit")?,
        security_out: option(&parsed, "out"),
        security_keyring: option(&parsed, "keyring"),
        security_keyring_provider: option(&parsed, "keyring-provider"),
        security_key_id: option(&parsed, "key-id"),
        auth_server_app_password: option(&parsed, "auth-server-app"),
        status_json: parsed.has_flag("json"),
        production: parsed.has_flag("production"),
        confirm_restore: parsed.has_flag("confirm-restore"),
        dry_run: parsed.has_flag("dry-run"),
        purge: parsed.has_flag("purge"),
        watch: parsed.has_flag("watch"),
        only_one: optional_bool(&parsed, "only-one", "no-only-one")?,
        kill_others: optional_bool(&parsed, "kill-others", "no-kill-others")?,
        ..CliArgs::default()
    };
    if parsed.has_flag("help") {
        cli.command = Some(RuntimeCliCommand::Help);
        cli.help_path = parsed.command_path().to_vec();
        return Ok(cli);
    }
    if parsed.has_flag("version") {
        cli.command = Some(RuntimeCliCommand::Version);
        return Ok(cli);
    }
    map_command(&parsed, &mut cli)?;
    Ok(cli)
}

fn map_command(parsed: &ParsedCli, cli: &mut CliArgs) -> Result<(), CliError> {
    let (command, action) = match parsed.command_path() {
        [] => return Ok(()),
        [name] if name == "help" => {
            cli.help_path = parsed.positionals().to_vec();
            (RuntimeCliCommand::Help, None)
        }
        [name] if name == "server" => (RuntimeCliCommand::Server, None),
        [name] if name == "status" => (RuntimeCliCommand::Status, None),
        [name] if name == "health" => (RuntimeCliCommand::Health, None),
        [name] if name == "doctor" => (RuntimeCliCommand::Doctor, None),
        [group, name] if group == "config" && name == "validate" => {
            (RuntimeCliCommand::ConfigValidate, None)
        }
        [name] if name == "diagnostics" => (RuntimeCliCommand::Diagnostics, None),
        [name] if name == "export" => (RuntimeCliCommand::Export, None),
        [name] if name == "version" => (RuntimeCliCommand::Version, None),
        [name] if name == "build-info" => (RuntimeCliCommand::BuildInfo, None),
        [name] if name == "first-run" => (RuntimeCliCommand::FirstRun, None),
        [name] if name == "run" => (RuntimeCliCommand::Run, None),
        [name] if name == "last-run" => (RuntimeCliCommand::LastRun, None),
        [name] if name == "paths" => (RuntimeCliCommand::Paths, None),
        [name] if name == "migrate" => (RuntimeCliCommand::UpdateRequired, None),
        [name] if name == "backup" => (RuntimeCliCommand::Backup, None),
        [group, action] if group == "backup" => (RuntimeCliCommand::Backup, Some(action)),
        [name] if name == "sync" => (RuntimeCliCommand::Sync, None),
        [group, action] if group == "sync" => (RuntimeCliCommand::Sync, Some(action)),
        [name] if name == "vault" => (RuntimeCliCommand::Vault, None),
        [group, action] if group == "token" => (token_command(action)?, None),
        [group, action] if group == "idempotency" && action == "compact" => {
            (RuntimeCliCommand::IdempotencyCompact, None)
        }
        [name] if name == "supervisor" => (RuntimeCliCommand::Supervisor, None),
        [security, secret, action] if security == "security" && secret == "secret" => {
            cli.security_action = Some(secret.clone());
            cli.security_secret_action = Some(action.clone());
            (RuntimeCliCommand::Security, None)
        }
        [name] if name == "completions" => {
            cli.completion_shell = shell(parsed.positionals().first())?;
            (RuntimeCliCommand::Completions, None)
        }
        [name] if name == "complete" => {
            parse_completion_request(parsed, cli)?;
            (RuntimeCliCommand::Complete, None)
        }
        path => {
            return Err(CliError::new(
                CliErrorKind::InvalidSpecification,
                format!("unmapped command path `{}`", path.join(" ")),
            ))
        }
    };
    cli.command = Some(command);
    if command == RuntimeCliCommand::Backup {
        cli.backup_action = action.cloned();
    } else if command == RuntimeCliCommand::Sync {
        cli.sync_action = action.cloned();
    }
    Ok(())
}

fn token_command(action: &str) -> Result<RuntimeCliCommand, CliError> {
    match action {
        "command" => Ok(RuntimeCliCommand::TokenCommand),
        "sync" => Ok(RuntimeCliCommand::TokenSync),
        "query" => Ok(RuntimeCliCommand::TokenQuery),
        _ => Err(CliError::new(
            CliErrorKind::InvalidSpecification,
            format!("unmapped token command `{action}`"),
        )),
    }
}

fn parse_completion_request(parsed: &ParsedCli, cli: &mut CliArgs) -> Result<(), CliError> {
    cli.completion_shell = shell(parsed.positionals().first())?;
    let cursor = parsed
        .positionals()
        .get(1)
        .ok_or_else(|| CliError::new(CliErrorKind::MissingValue, "missing completion cursor"))?
        .parse::<u64>()
        .map_err(|_| CliError::new(CliErrorKind::InvalidValue, "invalid completion cursor"))?;
    cli.completion_cursor_word = Some(usize::try_from(cursor).map_err(|_| {
        CliError::new(
            CliErrorKind::InvalidValue,
            "completion cursor exceeds platform limits",
        )
    })?);
    cli.completion_words = parsed.positionals()[2..].to_vec();
    Ok(())
}

fn shell(value: Option<&String>) -> Result<Option<Shell>, CliError> {
    let value = value
        .ok_or_else(|| CliError::new(CliErrorKind::MissingValue, "missing completion shell"))?;
    Shell::parse(value).map(Some).ok_or_else(|| {
        CliError::new(
            CliErrorKind::InvalidValue,
            format!("unsupported completion shell `{value}`"),
        )
    })
}

fn option(parsed: &ParsedCli, name: &str) -> Option<String> {
    parsed.option_value(name).map(str::to_string)
}

fn number(parsed: &ParsedCli, name: &str) -> Result<Option<u64>, CliError> {
    parsed
        .option_value(name)
        .map(|value| {
            value.parse::<u64>().map_err(|_| {
                CliError::new(
                    CliErrorKind::InvalidValue,
                    format!("invalid value for `--{name}`"),
                )
            })
        })
        .transpose()
}

fn optional_bool(
    parsed: &ParsedCli,
    positive: &str,
    negative: &str,
) -> Result<Option<bool>, CliError> {
    if parsed.has_flag(negative) {
        return Ok(Some(false));
    }
    if !parsed.has_flag(positive) {
        return Ok(None);
    }
    parsed
        .option_value(positive)
        .map_or(Ok(Some(true)), |value| {
            value.parse::<bool>().map(Some).map_err(|_| {
                CliError::new(
                    CliErrorKind::InvalidValue,
                    format!("invalid value for `--{positive}`"),
                )
            })
        })
}

fn contains_removed_input(words: &[String]) -> bool {
    words.iter().any(|word| {
        REMOVED_OPTIONS
            .iter()
            .any(|removed| word == removed || word.starts_with(&format!("{removed}=")))
    })
}

fn compatibility_error(args: &[String], error: &CliError) -> String {
    let candidate = match error.kind() {
        CliErrorKind::UnknownOption => args.iter().skip(1).rev().find(|word| word.starts_with('-')),
        CliErrorKind::UnexpectedArgument => args
            .iter()
            .skip(1)
            .rev()
            .find(|word| !word.starts_with('-')),
        _ => None,
    };
    candidate.cloned().unwrap_or_else(|| error.to_string())
}
