# Exemplo intermediário: arquivo, arquivo histórico e filtros

Este cenário registra operações normais no terminal e em JSONL, aumenta o
detalhe somente para `sync` e mantém a pasta principal pequena.

```rust
use appcore_log::{
    FileArchiveConfig, FileSinkConfig, LogOutputMode, LogPolicy, LoggerConfig,
    Verbosity, LOG_SIZE_8_MIB,
};

fn main() -> Result<(), appcore_log::LogConfigError> {
    // A aplicação permanece em V4; apenas sync aceita diagnóstico até V8.
    let mut policy = LogPolicy::new(Verbosity::V4);
    policy.set_component("sync", Verbosity::V8);

    let logger = LoggerConfig {
        policy,
        output: LogOutputMode::TerminalAndFile,
        file: Some(FileSinkConfig {
            // O nome é livre e a aplicação deve preparar o diretório.
            path: "logs/application.jsonl".into(),
            max_bytes: LOG_SIZE_8_MIB,
            sync_each_write: false,
            retention: 2,
            archive: Some(FileArchiveConfig {
                directory: "logs/archive".into(),
                max_files: 120,
            }),
        }),
        ..LoggerConfig::default()
    }
    .build()?;

    let application = logger.dispatcher().event(0, "application");
    let sync = logger.dispatcher().event(1, "sync.transport");

    application.info("aplicação pronta");

    // V7 aparece porque sync.transport herda a política de sync.
    sync.verbosity(7).debug("lote de replicação enviado");

    // O override é imutável: este evento volta ao V4 do builder.
    sync.warn("peer respondeu com atraso");

    let totals = logger.dispatcher().stats();
    let destinations = logger.dispatcher().sink_stats();

    assert_eq!(totals.sink_failures, 0);
    assert_eq!(destinations.len(), 2);

    Ok(())
}
```

Depois das rotações, a estrutura fica semelhante a:

```text
logs/
├── application.jsonl
├── application.jsonl.1
├── application.jsonl.2
└── archive/
    └── 2026/
        └── 09/
            └── application-00000001756944000000-0000.jsonl
```

O JSONL contém somente eventos que passaram pelo filtro e pela sanitização. Para
paths e secrets, construa `LogEvent` com `.path(...)` e `.secret(...)`. Para
conteúdo realmente Sensitive, use o exemplo separado `sensitive_diagnostics`;
nunca envie esse conteúdo ao arquivo JSONL comum.

Execute o exemplo de arquivo mantido no crate:

```shell
cargo run -p appcore-log --example file_logging
```

O resultado fica em `target/appcore-log-example/application.jsonl`. O diretório
`target` já é ignorado pelo Git e pode ser removido pelo clean do Cargo.

Quando I/O durável não puder bloquear o produtor, use o exemplo do wrapper
limitado explícito:

```shell
cargo run -p appcore-log --example async_file
```

Ele imprime o path absoluto de `target/appcore-log-example/async.jsonl`, limita
a fila por eventos e bytes e chama `shutdown` antes de sair. Saturação é
reportada em vez de esperar ou aumentar memória.

Voltar ao [guia](../guide.pt.md).
