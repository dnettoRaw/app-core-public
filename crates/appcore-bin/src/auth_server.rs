// =============================================================================
//        #######
//     ###       ###     F: auth_server.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/06 22:13:35 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Optional companion binary for future remote key workflows.

use crate::auth_server_grant::{issue_auth_grant, DEFAULT_AUTH_GRANT_TTL_MS};
use crate::auth_server_network::{
    run_auth_server_serve, AuthServerServeOptions, DEFAULT_AUTH_BIND,
};
use crate::bootstrap::now_ms;
use crate::constants::{default_app_version, AUTH_SERVER_BIN};
use appcore_args::{
    render_dynamic_completion_script, ArgumentSpec, CliParser, CliSpec, CommandSpec,
    CompletionEngine, CompletionRequest, HelpRenderer, OptionSpec, ParsedCli, RawArgs, Shell,
    ValueType,
};
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct AuthServerOptions {
    secret_path: Option<String>,
    data_secret_path: Option<String>,
    transport_secret_path: Option<String>,
    bind: Option<String>,
    resource: Option<String>,
    ttl_ms: Option<u64>,
    auto_restart: bool,
}

/// Parses and executes the auth-server command from the process environment.
pub fn run_auth_server_env() -> Result<(), String> {
    let args = RawArgs::from_env().map_err(|error| error.to_string())?;
    run_auth_server_raw(&args)
}

pub fn run_auth_server_cli(args: &[String]) -> Result<(), String> {
    let args = RawArgs::parse(args.iter().skip(1).cloned()).map_err(|error| error.to_string())?;
    run_auth_server_raw(&args)
}

fn run_auth_server_raw(args: &RawArgs) -> Result<(), String> {
    let spec = auth_server_spec();
    let parsed = CliParser::new(&spec)
        .parse(args)
        .map_err(|error| error.to_string())?;
    if parsed.has_flag("help") {
        return print_help(&spec, parsed.command_path());
    }
    if parsed.has_flag("version") {
        println!("{}", default_app_version());
        return Ok(());
    }
    match parsed.command_path() {
        [] => print_help(&spec, &[]),
        [name] if name == "help" => print_help(&spec, parsed.positionals()),
        [name] if name == "version" => {
            println!("{}", default_app_version());
            Ok(())
        }
        [name] if name == "status" => {
            let options = parse_options(&parsed)?;
            print_status(&options);
            Ok(())
        }
        [name] if name == "grant" => {
            let options = parse_options(&parsed)?;
            print_grant(&options)
        }
        [name] if name == "serve" => {
            let options = parse_options(&parsed)?;
            run_auth_server_serve(serve_options(&options)?)
        }
        [name] if name == "completions" => print_completion_script(parsed.positionals()),
        [name] if name == "complete" => print_completion_candidates(&spec, parsed.positionals()),
        path => Err(format!("unmapped auth-server command: {}", path.join(" "))),
    }
}

fn auth_server_spec() -> CliSpec {
    CliSpec::new(AUTH_SERVER_BIN)
        .about("Optional local AppCore auth-server companion.")
        .version(default_app_version())
        .option(
            OptionSpec::flag("help")
                .short('h')
                .terminal(true)
                .about("Show help."),
        )
        .option(
            OptionSpec::flag("version")
                .short('V')
                .terminal(true)
                .about("Print the host version."),
        )
        .option(path_option("secret", "Shared secret path."))
        .option(path_option("data-secret", "Data-encryption secret path."))
        .option(path_option(
            "transport-secret",
            "Grant transport secret path.",
        ))
        .option(
            OptionSpec::value("bind")
                .value_name("ADDRESS")
                .about("Listener address."),
        )
        .option(
            OptionSpec::value("resource")
                .value_name("NAME")
                .about("Grant resource name."),
        )
        .option(
            OptionSpec::value("ttl-ms")
                .value_name("MILLISECONDS")
                .value_type(ValueType::U64)
                .about("Grant lifetime."),
        )
        .option(OptionSpec::flag("auto-restart").about("Enable bounded listener restart."))
        .command(
            CommandSpec::new("help")
                .about("Show help for a command.")
                .argument(ArgumentSpec::new("command").multiple(true)),
        )
        .command(CommandSpec::new("version").about("Print the host version."))
        .command(CommandSpec::new("status").about("Inspect local secret files."))
        .command(CommandSpec::new("grant").about("Issue a short-lived transport grant."))
        .command(CommandSpec::new("serve").about("Run the local grant listener."))
        .command(
            CommandSpec::new("completions")
                .about("Print a shell completion script.")
                .argument(shell_argument()),
        )
        .command(
            CommandSpec::new("complete")
                .hidden(true)
                .argument(shell_argument())
                .argument(
                    ArgumentSpec::new("cursor")
                        .required(true)
                        .value_type(ValueType::U64),
                )
                .argument(ArgumentSpec::new("words").multiple(true)),
        )
}

fn path_option(name: &'static str, about: &'static str) -> OptionSpec {
    OptionSpec::value(name).value_name("PATH").about(about)
}

fn shell_argument() -> ArgumentSpec {
    ArgumentSpec::new("shell")
        .required(true)
        .possible_value("bash")
        .possible_value("zsh")
        .possible_value("fish")
        .possible_value("powershell")
}

fn print_help(spec: &CliSpec, path: &[String]) -> Result<(), String> {
    let path = path.iter().map(String::as_str).collect::<Vec<_>>();
    let help = HelpRenderer::new(spec)
        .render(&path)
        .map_err(|error| error.to_string())?;
    print!("{help}");
    Ok(())
}

fn print_completion_script(positionals: &[String]) -> Result<(), String> {
    let shell = parse_shell(positionals.first())?;
    print!(
        "{}",
        render_dynamic_completion_script(AUTH_SERVER_BIN, &["complete"], shell)
            .map_err(|error| error.to_string())?
    );
    Ok(())
}

fn print_completion_candidates(spec: &CliSpec, positionals: &[String]) -> Result<(), String> {
    let _shell = parse_shell(positionals.first())?;
    let cursor = positionals
        .get(1)
        .ok_or_else(|| "missing completion cursor".to_string())?
        .parse::<usize>()
        .map_err(|error| format!("invalid completion cursor: {error}"))?;
    for candidate in CompletionEngine::new(spec)
        .complete(&CompletionRequest::new(positionals[2..].to_vec(), cursor))
    {
        println!("{}", candidate.value());
    }
    Ok(())
}

fn parse_shell(value: Option<&String>) -> Result<Shell, String> {
    value
        .and_then(|value| Shell::parse(value))
        .ok_or_else(|| "missing or unsupported completion shell".to_string())
}

fn print_status(options: &AuthServerOptions) {
    println!("auth_server: installed");
    println!("network_mode: disabled");
    println!("auto_restart: {}", options.auto_restart);
    println!("secret_file: {}", secret_file_status(options));
    println!(
        "transport_secret_file: {}",
        transport_secret_file_status(options)
    );
}

fn print_grant(options: &AuthServerOptions) -> Result<(), String> {
    let secret = required_transport_secret(options)?;
    let resource = required_option(&options.resource, "--resource")?;
    let ttl_ms = options.ttl_ms.unwrap_or(DEFAULT_AUTH_GRANT_TTL_MS);
    let grant = issue_auth_grant(Path::new(secret), resource, ttl_ms, now_ms())?;
    println!("{grant}");
    Ok(())
}

fn parse_options(parsed: &ParsedCli) -> Result<AuthServerOptions, String> {
    Ok(AuthServerOptions {
        secret_path: parsed.option_value("secret").map(str::to_string),
        data_secret_path: parsed.option_value("data-secret").map(str::to_string),
        transport_secret_path: parsed.option_value("transport-secret").map(str::to_string),
        bind: parsed.option_value("bind").map(str::to_string),
        resource: parsed.option_value("resource").map(str::to_string),
        ttl_ms: parsed
            .option_value("ttl-ms")
            .map(str::parse::<u64>)
            .transpose()
            .map_err(|error| format!("invalid --ttl-ms: {error}"))?,
        auto_restart: parsed.has_flag("auto-restart"),
    })
}

fn required_option<'a>(value: &'a Option<String>, name: &str) -> Result<&'a str, String> {
    value
        .as_deref()
        .ok_or_else(|| format!("missing required {name}"))
}

fn required_transport_secret(options: &AuthServerOptions) -> Result<&str, String> {
    options
        .transport_secret_path
        .as_deref()
        .or(options.secret_path.as_deref())
        .ok_or_else(|| "missing required --transport-secret".to_string())
}

fn required_data_secret(options: &AuthServerOptions) -> Result<&str, String> {
    options
        .data_secret_path
        .as_deref()
        .or(options.secret_path.as_deref())
        .ok_or_else(|| "missing required --data-secret".to_string())
}

fn serve_options(options: &AuthServerOptions) -> Result<AuthServerServeOptions, String> {
    Ok(AuthServerServeOptions {
        data_secret_path: required_data_secret(options)?.to_string(),
        transport_secret_path: required_transport_secret(options)?.to_string(),
        bind: options
            .bind
            .clone()
            .unwrap_or_else(|| DEFAULT_AUTH_BIND.to_string()),
        auto_restart: options.auto_restart,
    })
}

fn secret_file_status(options: &AuthServerOptions) -> &'static str {
    match options.secret_path.as_deref() {
        Some(path) if Path::new(path).exists() => "present",
        Some(_) => "missing",
        None => "not-configured",
    }
}

fn transport_secret_file_status(options: &AuthServerOptions) -> &'static str {
    match options.transport_secret_path.as_deref() {
        Some(path) if Path::new(path).exists() => "present",
        Some(_) => "missing",
        None => "not-configured",
    }
}

#[cfg(test)]
#[path = "auth_server_tests.rs"]
mod tests;
