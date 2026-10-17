# appcore-args

Release candidate **`appcore-args 1.0.3-rc.1`**. Selecionar esta RC explicitamente; não substitui versões estáveis anteriores nem certifica um deployment. Quando existem, as dependências AppCore internas estão fixadas nesta rodada RC.

`RawArgs::from_env` valida cada palavra antes de retê-la e para no primeiro
argumento inválido. Não coleta o restante de argv em um buffer do AppCore antes
de aplicar os limites de quantidade e bytes.

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

## Documentação estável

ID estável: **ACR-001**. Consulte o
[guia complementar de arquitetura e integração](https://wiki.appcore.dnettoraw.com/pt/crates/id/acr-001). Esse ID permanente
continua válido se a página da wiki mudar.

## Configuração declarativa e paths nativos

Adições de fonte ainda não publicadas: `OptionSpec::env`, `default_value`,
`value_kind` e `exclusive_group`. Precedência: argv, ambiente declarado, depois
um valor padrão. Flags de env/default aceitam apenas `true` ou `false`; false
significa ausente. Obrigatoriedade, conflitos e requisitos são verificados após
resolução. Grupos aceitam no máximo um membro; `requires("source")` pode exigir
um membro desse grupo. Env/default compartilham budgets com argv. Help terminal
não consulta ambiente, mas valida a spec. Erros de env/default e Debug de opções
ocultam valores. Help/completion não consultam valores do ambiente.

Use `ValueKind::Path` para path nativo não vazio e `ValueKind::SocketAddr` para
IP/porta numéricos, incluindo IPv6 entre colchetes, sem DNS. `ValueType` mantém
o enum atual. `RawArgs` permanece UTF-8; `RawOsArgs::from_env` e
`CliParser::parse_os` preservam paths e passthrough nativos. Texto, nomes e
endereços não UTF-8 falham. Paths não são canonicalizados nem precisam existir.
`option_path` retorna `PathBuf`; `option_socket_addr` retorna endereço tipado.
`parse_with_env` e `parse_os_with_env` permitem injeção determinística.

```rust
use appcore_args::{CliSpec, CliParser, OptionSpec, RawOsArgs, ValueKind};
let spec = CliSpec::new("node")
    .option(OptionSpec::value("data-dir").env("NODE_DATA_DIR")
        .default_value("./data").value_kind(ValueKind::Path))
    .option(OptionSpec::value("bind").env("NODE_BIND")
        .default_value("127.0.0.1:9000").value_kind(ValueKind::SocketAddr))
    .option(OptionSpec::value("descriptor").exclusive_group("source"))
    .option(OptionSpec::value("catalog").exclusive_group("source"))
    .option(OptionSpec::flag("serve").requires("source"));
let raw = RawOsArgs::parse([std::ffi::OsString::from("--bind=[::1]:9000")])?;
let parsed = CliParser::new(&spec).parse_os_with_env(&raw, |_| None)?;
assert!(parsed.option_path("data-dir").is_some());
assert_eq!(parsed.option_socket_addr("bind")?.unwrap().port(), 9000);
# Ok::<(), appcore_args::CliError>(())
```
