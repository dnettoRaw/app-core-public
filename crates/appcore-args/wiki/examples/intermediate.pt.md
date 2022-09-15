# CLI intermediaria com `appcore-args`

Autor: [dnettoRaw](https://github.com/dnettoRaw)

[English](intermediate.en.md) | [Français](intermediate.fr.md) |
[Exemplo minimo](basic.pt.md) | [Guia](../guide.pt.md)

Esta CLI de backup deriva validacao, ajuda e autocomplete de uma unica
especificacao. A execucao continua sendo codigo explicito da aplicacao.

```rust
use appcore_args::{
    render_dynamic_completion_script, ArgumentSpec, CliParser, CliSpec,
    CommandSpec, CompletionEngine, CompletionRequest, HelpRenderer, OptionSpec,
    RawArgs, Shell, ValueType,
};
use std::error::Error;

fn spec() -> CliSpec {
    CliSpec::new("backupctl")
        .about("Create and inspect bounded backups.")
        .version("1.0.0")
        .command_required(true)
        .option(OptionSpec::flag("help").short('h').terminal(true))
        .command(
            CommandSpec::new("create")
                .option(OptionSpec::value("source").required(true))
                .option(OptionSpec::flag("dry-run")),
        )
        .command(CommandSpec::new("list"))
        .command(
            CommandSpec::new("completions").argument(
                ArgumentSpec::new("shell")
                    .required(true)
                    .possible_value("bash")
                    .possible_value("zsh")
                    .possible_value("fish")
                    .possible_value("powershell"),
            ),
        )
        .command(
            CommandSpec::new("complete")
                .hidden(true)
                .argument(ArgumentSpec::new("shell").required(true))
                .argument(
                    ArgumentSpec::new("cursor")
                        .required(true)
                        .value_type(ValueType::U64),
                )
                .argument(ArgumentSpec::new("words").required(true).multiple(true)),
        )
}

fn main() -> Result<(), Box<dyn Error>> {
    let spec = spec();
    let parsed = CliParser::new(&spec).parse(&RawArgs::from_env()?)?;

    if parsed.has_flag("help") {
        let path = parsed.command_path().iter().map(String::as_str).collect::<Vec<_>>();
        print!("{}", HelpRenderer::new(&spec).render(&path)?);
        return Ok(());
    }

    match parsed.command_path() {
        [command] if command == "create" => {
            let source = parsed.option_value("source").ok_or("missing --source")?;
            println!("source={source} dry_run={}", parsed.has_flag("dry-run"));
        }
        [command] if command == "list" => println!("no backups yet"),
        [command] if command == "completions" => {
            let shell = parsed
                .positionals()
                .first()
                .and_then(|value| Shell::parse(value))
                .ok_or("invalid shell")?;
            print!("{}", render_dynamic_completion_script("backupctl", &["complete"], shell)?);
        }
        [command] if command == "complete" => {
            let [shell, cursor, words @ ..] = parsed.positionals() else {
                return Err("missing completion arguments".into());
            };
            Shell::parse(shell).ok_or("invalid shell")?;
            let request = CompletionRequest::new(words.to_vec(), cursor.parse()?);
            for candidate in CompletionEngine::new(&spec).complete(&request) {
                println!("{}", candidate.value());
            }
        }
        _ => return Err("unmapped command".into()),
    }
    Ok(())
}
```

O script gerado chama o comando oculto `complete`. Candidatos sao dados
impressos um por linha; a entrada do usuario nunca e inserida no codigo shell.
