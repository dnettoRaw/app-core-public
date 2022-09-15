# appcore-args

```text
       #######
    ###       ###
   ##   ## ##   ##
        ## ##
   ##   ## ##   ##
     ###########
```

Autor: [dnettoRaw](https://github.com/dnettoRaw)

[English](README.en.md) | [Français](README.fr.md)

Primitivas sem dependências e multiplataforma para parsing de linha de
comando, ajuda, validação e completion de shell em executáveis AppCore.

O crate possui SemVer independente, não tem dependências e pode ser consumido
por qualquer executável Rust sem o AppCore Runtime.

`appcore-args` oferece comandos aninhados, opções herdadas, argumentos tipados e
limitados, erros determinísticos, ajuda gerada, candidatos de completion e
scripts dinâmicos para Bash, Zsh, Fish e PowerShell. Ele não executa
comportamento do Runtime e não usa código `unsafe`.

```rust
use appcore_args::{ArgumentSpec, CliParser, CliSpec, CommandSpec, OptionSpec, RawArgs};

let spec = CliSpec::new("demo")
    .option(OptionSpec::flag("verbose").short('v'))
    .command(CommandSpec::new("run").argument(
        ArgumentSpec::new("mode").required(true).possible_value("safe"),
    ));
let parsed = CliParser::new(&spec)
    .parse(&RawArgs::parse(["run", "safe", "-v"])? )?;

assert_eq!(parsed.command_path(), &["run"]);
assert!(parsed.has_flag("verbose"));
# Ok::<(), Box<dyn std::error::Error>>(())
```

Veja o [guia em português](wiki/guide.pt.md), o
[exemplo minimo](wiki/examples/basic.pt.md) e o
[exemplo intermediario](wiki/examples/intermediate.pt.md). A documentação da API está no
[docs.rs](https://docs.rs/appcore-args).

Licença: MIT.
