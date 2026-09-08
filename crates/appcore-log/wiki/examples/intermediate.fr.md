# Exemple intermédiaire : fichiers, archive et filtres

Ce scénario écrit les opérations normales dans le terminal et en JSONL,
augmente le détail uniquement pour `sync` et garde le dossier actif petit.

```rust
use appcore_log::{
    FileArchiveConfig, FileSinkConfig, LogOutputMode, LogPolicy, LoggerConfig,
    Verbosity, LOG_SIZE_8_MIB,
};

fn main() -> Result<(), appcore_log::LogConfigError> {
    // L'application reste en V4 ; seul sync accepte les diagnostics jusqu'à V8.
    let mut policy = LogPolicy::new(Verbosity::V4);
    policy.set_component("sync", Verbosity::V8);

    let logger = LoggerConfig {
        policy,
        output: LogOutputMode::TerminalAndFile,
        file: Some(FileSinkConfig {
            // Le nom est libre ; l'application prépare le dossier.
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

    application.info("application prête");

    // V7 est visible car sync.transport hérite de la politique sync.
    sync.verbosity(7).debug("lot de réplication envoyé");

    // La surcharge est immuable ; cet événement réutilise V4.
    sync.warn("réponse du peer retardée");

    let totals = logger.dispatcher().stats();
    let destinations = logger.dispatcher().sink_stats();

    assert_eq!(totals.sink_failures, 0);
    assert_eq!(destinations.len(), 2);

    Ok(())
}
```

Après rotation, l'arborescence ressemble à :

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

Le JSONL ne contient que les événements filtrés et assainis. Pour paths et
secrets, construisez `LogEvent` avec `.path(...)` et `.secret(...)`. Pour du
contenu Sensitive, utilisez l'exemple `sensitive_diagnostics` ; ne l'envoyez
jamais vers un JSONL ordinaire.

Exécutez l'exemple fichier maintenu :

```shell
cargo run -p appcore-log --example file_logging
```

Le résultat se trouve dans `target/appcore-log-example/application.jsonl`.
Git ignore déjà `target` et Cargo clean peut le supprimer.

Lorsque l'I/O durable ne doit pas bloquer le producteur, utilisez l'exemple du
wrapper borné explicite :

```shell
cargo run -p appcore-log --example async_file
```

Il affiche le chemin absolu de `target/appcore-log-example/async.jsonl`, borne
la file par événements et octets, puis appelle `shutdown` avant de sortir. La
saturation est signalée au lieu d'attendre ou d'augmenter la mémoire.

Retour au [guide](../guide.fr.md).
