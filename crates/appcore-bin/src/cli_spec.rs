// =============================================================================
//        #######
//     ###       ###     F: cli_spec.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/19 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/19 00:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use appcore_args::{ArgumentSpec, CliSpec, CommandSpec, OptionSpec, ValueType};

pub(super) fn runtime_spec(name: &str, about: &str, version: &str) -> CliSpec {
    commands().into_iter().fold(
        options().into_iter().fold(
            CliSpec::new(name)
                .about(about)
                .version(version)
                .command_required(false),
            CliSpec::option,
        ),
        CliSpec::command,
    )
}

fn commands() -> Vec<CommandSpec> {
    vec![
        command("help", "Show help for a command.")
            .argument(ArgumentSpec::new("command").multiple(true)),
        command("server", "Start the Runtime host."),
        command("status", "Print Runtime status."),
        command("health", "Run Runtime health checks."),
        command("doctor", "Validate service graph and deployment policy."),
        group(
            "config",
            "Manage Runtime configuration.",
            vec![command("validate", "Validate versioned manifests.")],
            true,
        ),
        command("diagnostics", "Print redacted Runtime diagnostics."),
        command("export", "Export diagnostics and in-memory audit."),
        command("version", "Print the host version."),
        command("build-info", "Print embedded build metadata."),
        command("first-run", "Create local manifests, secret and markers."),
        command("run", "Run from the first-run deployment manifest."),
        command("last-run", "Inspect or remove local application data."),
        command("paths", "Print platform-local AppCore paths."),
        command("migrate", "Reject the removed migration interface.").hidden(true),
        group(
            "backup",
            "Manage cold storage backups.",
            vec![
                command("create", "Create a snapshot backup."),
                command("verify", "Verify a snapshot backup."),
                command("restore", "Restore a snapshot backup."),
                command("drill", "Run a bounded restore drill."),
            ],
            false,
        ),
        group(
            "sync",
            "Inspect or push conservative replication.",
            vec![
                command("status", "Print synchronization status."),
                command("push", "Push available records to followers."),
            ],
            false,
        ),
        command("vault", "Print the external vault contract status."),
        group(
            "token",
            "Issue signed Runtime tokens.",
            vec![
                command("command", "Issue a command token."),
                command("sync", "Issue a synchronization token."),
                command("query", "Issue a query token."),
            ],
            true,
        ),
        group(
            "idempotency",
            "Maintain the idempotency store.",
            vec![command("compact", "Compact expired reservations.")],
            true,
        ),
        command("supervisor", "Run the Runtime process watchdog."),
        group(
            "security",
            "Manage Runtime security material.",
            vec![group(
                "secret",
                "Inspect or rotate owner-controlled secrets.",
                vec![
                    command("status", "Inspect secret metadata."),
                    command("rotate", "Create a replacement secret file."),
                    command("keyring-init", "Initialize an owner-only V1 keyring."),
                    command("keyring-rotate", "Rotate the active key."),
                    command("keyring-status", "Inspect active key metadata."),
                    command("keyring-recover", "Recover an unambiguous active pointer."),
                    command("keyring-revoke", "Revoke one key by identifier."),
                ],
                true,
            )],
            true,
        ),
        command("completions", "Print a shell completion script.").argument(shell_argument()),
        command("complete", "Return completion candidates.")
            .hidden(true)
            .argument(shell_argument())
            .argument(
                ArgumentSpec::new("cursor")
                    .required(true)
                    .value_type(ValueType::U64),
            )
            .argument(ArgumentSpec::new("words").multiple(true)),
    ]
}

fn options() -> Vec<OptionSpec> {
    vec![
        OptionSpec::flag("help")
            .short('h')
            .terminal(true)
            .about("Show help."),
        OptionSpec::flag("version")
            .short('V')
            .terminal(true)
            .about("Print the host version."),
        value("deployment", "PATH", "Deployment Manifest path."),
        value("file", "PATH", "Backup source file."),
        value("name", "NAME", "Backup name."),
        value("out", "PATH", "Output path."),
        value("keyring", "PATH", "Owner-controlled keyring root."),
        value("key-id", "ID", "Key identifier."),
        value(
            "auth-server-app",
            "PASSWORD",
            "Auth-server installation gate.",
        ),
        number("ttl-ms", "Token or key lifetime in milliseconds."),
        value("command", "NAME", "Command capability name."),
        value("query", "NAME", "Query capability name."),
        value("subject", "SUBJECT", "Token subject."),
        value("scope", "SCOPE", "Token scope."),
        number("max-restarts", "Maximum process restart count."),
        value(
            "child-args",
            "ARGS",
            "Arguments passed to the child process.",
        ),
        value("health-url", "URL", "Supervisor health endpoint."),
        number(
            "health-check-every-ticks",
            "Supervisor health-check interval in ticks.",
        ),
        number(
            "health-fail-limit",
            "Consecutive failed health checks before restart.",
        ),
        OptionSpec::flag("json").about("Print JSON output."),
        OptionSpec::flag("production").about("Validate the production profile."),
        OptionSpec::flag("confirm-restore").about("Confirm a destructive restore."),
        OptionSpec::flag("dry-run").about("Report changes without applying them."),
        OptionSpec::flag("purge").about("Remove local data and cache."),
        OptionSpec::flag("watch").about("Keep the Runtime server watching."),
        optional_bool("only-one", "Require a single local instance.").conflicts_with("no-only-one"),
        OptionSpec::flag("no-only-one")
            .conflicts_with("only-one")
            .about("Disable single-instance enforcement."),
        optional_bool("kill-others", "Stop conflicting local instances.")
            .conflicts_with("no-kill-others"),
        OptionSpec::flag("no-kill-others")
            .conflicts_with("kill-others")
            .about("Do not stop conflicting local instances."),
    ]
}

fn command(name: &'static str, about: &'static str) -> CommandSpec {
    CommandSpec::new(name).about(about)
}

fn group(
    name: &'static str,
    about: &'static str,
    children: Vec<CommandSpec>,
    required: bool,
) -> CommandSpec {
    children.into_iter().fold(
        command(name, about).command_required(required),
        CommandSpec::command,
    )
}

fn value(name: &'static str, value_name: &'static str, about: &'static str) -> OptionSpec {
    OptionSpec::value(name).value_name(value_name).about(about)
}

fn number(name: &'static str, about: &'static str) -> OptionSpec {
    value(name, "NUMBER", about).value_type(ValueType::U64)
}

fn optional_bool(name: &'static str, about: &'static str) -> OptionSpec {
    OptionSpec::value(name)
        .optional_value()
        .detached_optional_value(true)
        .value_name("BOOL")
        .value_type(ValueType::Bool)
        .about(about)
}

fn shell_argument() -> ArgumentSpec {
    ArgumentSpec::new("shell")
        .required(true)
        .possible_value("bash")
        .possible_value("zsh")
        .possible_value("fish")
        .possible_value("powershell")
}
