# Guia do AppCore Log

Este guia parte do caso mais simples e chega a arquivos rotativos, logs de
crash e diagnósticos criptografados. O logger é local ao objeto que o possui:
não existe configuração global, thread ou fila criada implicitamente.

## 1. Escolha onde escrever

`LoggerConfig::default()` usa `Terminal`, política segura e verbosidade V4.
Altere apenas `output` para escolher o comportamento:

| `LogOutputMode` | Comportamento |
|---|---|
| `Terminal` | Escreve texto no stdout. |
| `File` | Escreve uma linha JSON por evento. |
| `TerminalAndFile` | Envia o mesmo evento sanitizado aos dois destinos. |
| `Disabled` | Retorna antes de converter a mensagem ou chamar sinks. |
| `CrashOnly` | Guarda eventos num ring limitado e não cria arquivo normalmente. |

Os modos `File`, `TerminalAndFile` e `CrashOnly` exigem `file: Some(...)`.
Uma configuração inválida retorna `LogConfigError`; não existe fallback oculto.

```rust
use appcore_log::{LogOutputMode, LoggerConfig};

let logger = LoggerConfig {
    output: LogOutputMode::Terminal,
    ..LoggerConfig::default()
}
.build()
.expect("configuração válida");
```

## 2. Crie um logger reutilizável

O componente agrupa eventos e permite filtros específicos. Mantenha o builder
quando vários logs compartilham o mesmo componente e nível padrão.

```rust
use appcore_log::LoggerConfig;

let logger = LoggerConfig::default()
    .build()
    .expect("configuração válida");
let log = logger.dispatcher().event(0, "sync");

log.info("replicação iniciada");
log.warn("peer temporariamente indisponível");
log.error("checkpoint não foi persistido");
```

Em aplicações SDK, configure uma vez com `App::logging(config)` e obtenha o
builder com `app.logger()`.

## 3. Configure arquivo e rotação

Cada campo de `FileSinkConfig` tem uma responsabilidade:

| Campo | Para que serve |
|---|---|
| `path` | Diretório e nome do arquivo ativo. |
| `max_bytes` | Tamanho máximo de cada JSONL; deve ser maior que zero. |
| `sync_each_write` | Faz flush durável por evento quando `true`. |
| `retention` | Rotações mantidas ao lado do arquivo ativo, de 0 a 32. |
| `archive` | Destino opcional para rotações que saem da retenção ativa. |

`FileArchiveConfig.directory` recebe subpastas `AAAA/MM`. `max_files` limita o
histórico completo entre 1 e 10.000 arquivos.

```rust
use appcore_log::{
    FileArchiveConfig, FileSinkConfig, LogOutputMode, LoggerConfig,
    LOG_SIZE_8_MIB,
};

let logger = LoggerConfig {
    output: LogOutputMode::File,
    file: Some(FileSinkConfig {
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
.build()
.expect("configuração válida");
```

Nesse exemplo, a pasta principal mantém o ativo e duas rotações. Arquivos mais
antigos recebem nome com timestamp e sequência dentro de `archive/AAAA/MM`.
Use uma constante de `LOG_SIZE_1_MIB` a `LOG_SIZE_64_MIB` ou informe outro
`u64` não zero.

`sync_each_write: false` reduz syscalls e normalmente é a melhor escolha para
logs operacionais. Use `true` quando perder os últimos eventos após uma queda
do sistema for menos aceitável que o custo de I/O.

A aplicação deve criar o diretório pai do arquivo ativo antes do primeiro
evento. O sink cria as subpastas `AAAA/MM` do histórico quando necessário e
recusa destinos por link simbólico.

A serialização JSONL conta bytes sem alocar o payload antes de verificar
`max_bytes`, incluindo escapes e quebra de linha. Registros rejeitados não
rotacionam nem alteram arquivos. Os aceitos reservam o tamanho exato e são
serializados novamente: uma segunda passagem controla a alocação temporária.
Não é um teto de RSS; overhead do allocator e eventos do chamador são separados.

## 4. Entenda severidade e verbosidade

Severidade informa o impacto: `Trace`, `Debug`, `Info`, `Warn`, `Error` ou
`Critical`. Verbosidade informa quanto detalhe é necessário para ver o evento:

| Faixa | Uso típico |
|---|---|
| V1–V3 | Falhas críticas, erros importantes e warnings relevantes. |
| V4–V6 | Eventos essenciais, operação normal e fluxo. |
| V7–V9 | Debug técnico, I/O, timing e diagnóstico profundo. |

Uma política V4 aceita eventos V1 a V4. Para elevar apenas um subsistema:

```rust
use appcore_log::{LogPolicy, Verbosity};

let mut policy = LogPolicy::new(Verbosity::V4);
policy.set_component("sync", Verbosity::V8);
```

O override de `sync` também cobre `sync.transport`. Um override mais específico
vence. `log.verbosity(7).debug(...)` altera somente aquele evento.

## 5. Estruture dados sensíveis e paths

Use `LogEvent::field` para dados comuns, `LogEvent::path` para paths e
`LogEvent::secret` para credenciais. Safe e Diagnostic sanitizam o evento antes
de terminal, arquivo ou ring.

```rust
use appcore_log::{LogEvent, Severity, Verbosity};

let event = LogEvent::new(0, Severity::Info, Verbosity::V4, "storage", "aberto")
    .path("file", "/srv/app/data.json")
    .secret("token", "não deve aparecer");
```

Configure `PathAliases` para produzir valores como `<APP_ROOT>/data.json`.
Paths desconhecidos viram `<LOCAL_PATH>`. Não coloque secrets ou paths livres
dentro da mensagem; campos tipados são o limite confiável.

`Sensitivity::Diagnostic` aumenta contexto técnico, mas mantém a mesma
proteção. `Sensitivity::Sensitive` exige `SensitiveDntSink`, chave explícita e
DNT criptografado. O dispatcher recusa sinks comuns nesse modo e nunca faz
fallback plaintext.

## 6. Registre somente em crash

`CrashOnly` mantém um `RingBufferSink` sanitizado e limitado por
`crash_events` e `crash_bytes`. O arquivo permanece ausente até o dump:

```rust
use appcore_log::{FileSinkConfig, LogOutputMode, LoggerConfig, LOG_SIZE_2_MIB};

let logger = LoggerConfig {
    output: LogOutputMode::CrashOnly,
    file: Some(FileSinkConfig {
        path: "logs/crash.jsonl".into(),
        max_bytes: LOG_SIZE_2_MIB,
        sync_each_write: true,
        retention: 1,
        archive: None,
    }),
    crash_events: 128,
    crash_bytes: 512 * 1024,
    ..LoggerConfig::default()
}
.build()
.expect("configuração válida");

// Execute uma vez no boundary controlado de crash.
let _quantidade = logger.dump_crash().expect("dump gravado");
```

O crate não instala panic hook porque ownership de processo pertence à
aplicação ou ao deployment.

## 7. Observe falhas e teste

Eventos inválidos ou filtrados não chegam aos sinks. Falhas de escrita são
contadas sem tentar registrar outro log, evitando recursão:

- `stats()`: filtrados, inválidos, falhas e descartes gerais;
- `sink_stats()`: falhas separadas por destino;
- `FixedLogClock`: timestamps determinísticos em testes.

Limites públicos: 4 KiB por texto/identidade, 32 fields por evento, 128 bytes
por chave e 4 KiB por valor. Rings sempre exigem teto de quantidade e bytes.

Continue com o [exemplo básico](examples/basic.pt.md) ou o
[exemplo intermediário](examples/intermediate.pt.md).

## 8. Meça no hardware real

O bench nativo cobre emissão desligada/filtrada, sanitização, ring limitado,
JSONL buffered/durável e rotação com arquivo histórico:

```shell
cargo run -p appcore-dev -- bench --name appcore-log \
  --output target/appcore-log-benchmark.json
```

O JSON inclui p50/p95, CPU, pico e retenção de RSS, toolchain e hardware. Guarde
um baseline e use `appcore-dev bench compare` antes de aceitar uma otimização.

O sink síncrono de arquivo reutiliza uma única alça append. Seu mutex cobre
contagem de bytes, detecção de substituição, rotação, escrita e sync opcional.
Ele reabre após remoção, substituição, truncamento ou append externo; não cria
fila assíncrona nem altera a política de durabilidade configurada.

`AsyncSink` é a alternativa assíncrona explícita. Configure tetos de eventos e
bytes estimados retidos, passe-o como `LogSink` e chame `flush` ou `shutdown` na
fronteira de lifecycle dona. A admissão nunca espera e retorna `Capacity` na
saturação. O worker isola panic do sink, expõe contadores de entrega, falha e
rejeição e usa stack de 256 KiB. Shutdown só pode ser limitado quando o I/O do
sink interno também é limitado; drop não espera.

Os casos `jsonl_concurrent_buffered_batch_64` e
`jsonl_concurrent_synced_batch_64` usam quatro produtores compartilhando um
dispatcher e sink de arquivo, com 16 eventos cada por iteração. O tempo inclui
criação de eventos, sanitização, I/O e duas sincronizações por barreira a cada
lote; exclui criação/join das threads e preparo/remoção do diretório.
`serialized_slow_sink_batch_4` emite um evento por produtor em um sink
serializado por mutex, com espera de 1 ms por evento. Simula bloqueio, não
latência de disco físico. Os tempos são por lote, não por evento nem percentis
de latência de cada produtor. RSS inclui stacks das threads. Esses casos medem
a baseline síncrona; testes unitários, e não timing, provam limites durante a
entrega ativa e flush/shutdown explícito. Eles não provam durabilidade após crash.
