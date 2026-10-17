# Guia Do appcore-args

Release candidate **`appcore-args 1.0.3-rc.1`**. Selecionar esta RC explicitamente; não substitui versões estáveis anteriores nem certifica um deployment. Quando existem, as dependências AppCore internas estão fixadas nesta rodada RC.

`RawArgs::from_env` valida cada palavra antes de retê-la e para no primeiro
argumento inválido. Não coleta o restante de argv em um buffer do AppCore antes
de aplicar os limites de quantidade e bytes.

Autor: [dnettoRaw](https://github.com/dnettoRaw)

[English](guide.en.md) | [Français](guide.fr.md) |
[Exemplo minimo](examples/basic.pt.md) |
[Exemplo intermediario](examples/intermediate.pt.md)

## Responsabilidade

O crate possui especificações de comandos, leitura limitada de argumentos,
parsing determinístico, ajuda gerada, candidatos de completion e integração
dinâmica com shells. Os consumidores possuem a execução e todo comportamento
do Runtime.

Este é um crate autônomo com versão independente. Sua API pública não pode
importar contratos ou tipos de qualquer outro crate AppCore.

## Modelo De Comandos

- `CliSpec` e `CommandSpec` definem comandos aninhados, aliases, opções
  herdadas, posicionais e subcomandos obrigatórios.
- `OptionSpec` define nomes longos e curtos, valores proibidos, obrigatórios ou
  opcionais, repetição, requisitos e conflitos.
- Opções terminais como `--help` podem ignorar validações de entradas
  obrigatórias.
- `ArgumentSpec` define posicionais fixos ou um último posicional variável.
- `ValueType` valida texto, booleanos e inteiros com ou sem sinal.

Toda especificação é validada antes de parsing, ajuda ou completion. Nomes
inválidos, aliases duplicados, colisões de opções herdadas, relações
desconhecidas e layouts posicionais ambíguos falham de forma fechada.

## Fronteira De Entrada

`RawArgs::from_env` rejeita entradas não UTF-8 sem conversão com perda. Os
limites padrão são 1.024 palavras, 64 KiB por palavra e 1 MiB no total. Limites
customizados estão disponíveis em `RawArgs::parse_with_limits`. Bytes NUL são
sempre rejeitados.

O parser aceita `--name value`, `--name=value`, flags agrupadas como `-av`,
valores curtos anexados como `-oresult` ou `-o=result`, posicionais negativos
com sinal e passthrough após `--`. Valores opcionais aceitam apenas
`--name=value` ou valor curto anexado, evitando consumir o próximo posicional
de forma ambígua. O consumidor pode habilitar um valor opcional separado com
`detached_optional_value(true)`; use um tipo restritivo como `Bool` para
consumir apenas a próxima palavra válida.

Comandos, opções longas e valores enumerados desconhecidos incluem uma sugestão
próxima quando ela existe. O cálculo é limitado a entradas e candidatos de 128
bytes; valores maiores ainda retornam o erro tipado sem análise de similaridade.

## Ajuda E Completion

`HelpRenderer` e `CompletionEngine` consomem a mesma especificação validada do
parser. Entradas ocultas são omitidas, opções não repetíveis já usadas não são
sugeridas e valores possíveis viram candidatos de completion.

`render_dynamic_completion_script` suporta Bash, Zsh, Fish e PowerShell. Os
tokens do executável e do comando de completion são restritos antes da
interpolação. Sem candidato estrutural, as integrações preservam o completion
nativo de arquivos do shell.

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
