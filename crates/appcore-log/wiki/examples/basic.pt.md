# Exemplo básico: log seguro no terminal

Este exemplo cria um logger local, mantém um builder para o componente da
aplicação e escreve três severidades. A política padrão é Safe com verbosidade
V4.

```rust
use appcore_log::{LogOutputMode, LoggerConfig};

fn main() -> Result<(), appcore_log::LogConfigError> {
    // A configuração pertence à aplicação e falha explicitamente se inválida.
    let logger = LoggerConfig {
        output: LogOutputMode::Terminal,
        ..LoggerConfig::default()
    }
    .build()?;

    // Reutilizar o builder evita repetir o componente em cada chamada.
    let log = logger.dispatcher().event(0, "application");

    log.info("aplicação iniciada");
    log.warn("conexão mais lenta que o esperado");
    log.error("não foi possível salvar o documento");

    Ok(())
}
```

Saída aproximada:

```text
[Info][application][V4] aplicação iniciada
[Warn][application][V4] conexão mais lenta que o esperado
[Error][application][V4] não foi possível salvar o documento
```

O timestamp `0` deixa o exemplo determinístico; uma aplicação real pode usar
`SystemLogClock` com `event_now`. O logger não cria thread nem estado global.

Execute a versão mantida no crate:

```shell
cargo run -p appcore-log --example basic
```

Próximo passo: [exemplo intermediário](intermediate.pt.md).
