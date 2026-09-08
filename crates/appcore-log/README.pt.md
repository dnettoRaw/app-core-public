# appcore-log

[English](README.en.md) | [Français](README.fr.md)

`appcore-log` fornece logs operacionais estruturados, limitados e seguros para
o AppCore. Não existe logger global nem fila escondida, e nenhum worker em
background é criado sem a escolha explícita de `AsyncSink`. A aplicação escolhe
filtros, destinos, retenção e durabilidade.

## Início rápido

```rust
use appcore_log::{LogOutputMode, LoggerConfig};

fn main() -> Result<(), appcore_log::LogConfigError> {
    let logger = LoggerConfig {
        output: LogOutputMode::Terminal,
        ..LoggerConfig::default()
    }
    .build()?;

    let log = logger.dispatcher().event(0, "application");

    log.info("aplicação iniciada");
    log.warn("conexão lenta");

    Ok(())
}
```

Aplicações com `appcore-sdk` configuram o mesmo logger por `App::logging` e
escrevem com `app.log(...)` ou `app.logger()`.

## Modos de saída

| Modo | Execução normal | Arquivo obrigatório |
|---|---|---|
| `Terminal` | Texto legível no terminal | Não |
| `File` | JSONL estruturado e limitado | Sim |
| `TerminalAndFile` | Terminal e JSONL | Sim |
| `Disabled` | Sem sink nem conversão da mensagem | Não |
| `CrashOnly` | Ring sanitizado e limitado em memória | Sim, criado apenas por `dump_crash` |

`CrashOnly` não instala um panic handler. Chame `dump_crash` uma vez no boundary
controlado de crash da aplicação.

## Arquivos limitados

```rust
use appcore_log::{
    FileArchiveConfig, FileSinkConfig, LogOutputMode, LoggerConfig,
    LOG_SIZE_8_MIB,
};

fn main() -> Result<(), appcore_log::LogConfigError> {
    let logger = LoggerConfig {
        output: LogOutputMode::TerminalAndFile,
        file: Some(FileSinkConfig {
            path: "logs/runtime.jsonl".into(),
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

    logger.dispatcher().event(0, "application").info("pronta");

    Ok(())
}
```

A pasta ativa contém `runtime.jsonl`, `runtime.jsonl.1` e `runtime.jsonl.2`.
Rotações mais antigas vão para `archive/AAAA/MM` até o limite global
`max_files`. O nome do arquivo é definido pela aplicação.
Crie o diretório pai do arquivo ativo durante o setup; as pastas anuais e
mensais do arquivo histórico são criadas durante a rotação. Destinos de arquivo
e histórico recusam links simbólicos em vez de segui-los.

Use `sync_each_write: true` quando cada evento precisar chegar ao storage antes
do retorno. `false` evita esse syscall e favorece velocidade. Existem constantes
de `LOG_SIZE_1_MIB` até `LOG_SIZE_64_MIB`; qualquer `u64` não zero continua válido.

## Severidade, verbosidade e componentes

Severidade representa impacto, de `Trace` a `Critical`. Verbosidade representa
detalhe, de V1 a V9. Uma política V4 aceita V1 a V4; ela não significa
"severidade quatro". Overrides são hierárquicos: `sync` também cobre
`sync.transport`, salvo configuração mais específica.

## Limite de segurança

Políticas Safe e Diagnostic removem secrets tipados e criam aliases para paths
tipados antes dos sinks normais. Use `LogEvent::secret` e `LogEvent::path`; não
coloque credenciais na mensagem livre. Paths completos exigem decisão explícita.

Diagnósticos Sensitive exigem `Sensitivity::Sensitive` e um
`SensitiveDntSink` explícito. O conteúdo é autenticado e criptografado em DNT,
sem fallback para terminal, JSONL ou memória comum. Campos secret continuam
redigidos inclusive nesse modo.

## Limites e falhas

- Texto do evento: 4 KiB por campo textual ou de identidade.
- Fields estruturados: 32 por evento.
- Chaves: 128 bytes; valores: 4 KiB.
- Rotações ativas: no máximo 32.
- Arquivos históricos: de 1 a 10.000.
- Rings: limitados por quantidade e memória estimada.
- Falhas de sink: contabilizadas sem logging recursivo.

Use `stats()` para contadores gerais e `sink_stats()` para falhas por destino.
Use `FixedLogClock` em testes determinísticos.

Veja o [guia em português](wiki/guide.pt.md) e os exemplos executáveis em
[`examples/`](examples/).

Antes de alocar o payload JSONL, o sink conta os bytes serializados, incluindo
escapes e quebra de linha, contra `max_bytes`. Registros grandes demais retornam
`Capacity` antes de rotação ou escrita. Registros aceitos usam reserva de tamanho
exato e uma segunda serialização; overhead do allocator, eventos do chamador e
memória do filesystem ficam fora desse orçamento de payload.

Cada `FileSink` mantém uma única alça append aberta e acompanha o tamanho do
arquivo ativo sob o mutex existente. A alça é fechada antes da rotação e
reaberta se o path for removido, substituído, truncado ou receber escrita
externa. Continua não existindo fila escondida nem worker de flush em background.

Para uma fronteira assíncrona explícita, envolva um sink em `AsyncSink`. Seu
`emit` não bloqueante é limitado por `AsyncSinkConfig::max_events` e
`max_bytes`; saturação retorna `Capacity` e é contabilizada. `flush` espera os
eventos já admitidos e `shutdown` drena e faz join do único worker de 256 KiB.
O sink interno precisa concluir: I/O bloqueante arbitrário não pode ser
encerrado à força com segurança. Drop sem shutdown explícito nunca espera esse
I/O. A fila é opt-in e não altera os defaults de `LoggerConfig`.

## Benchmark

Execute os dez workloads com informações do hardware:

```shell
cargo run -p appcore-dev -- bench --name appcore-log \
  --output target/appcore-log-benchmark.json
```

O relatório registra distribuições de tempo, CPU, RSS máximo/retido e contexto
de CPU, RAM, GPU, sistema e filesystem. Compare relatórios compatíveis com
`appcore-dev bench compare`.

Os casos JSONL concorrentes medem 64 eventos por lote de quatro produtores,
com e sem sync por evento. O sink lento serializa quatro eventos com espera
simulada de 1 ms cada; não mede disco físico. Consulte os limites da medição
no guia.

## Documentação estável

ID estável: **ACR-027**. Consulte o
[guia complementar de arquitetura e integração](https://wiki.appcore.dnettoraw.com/pt/crates/id/acr-027). Esse ID permanente
continua válido se a página da wiki mudar.
