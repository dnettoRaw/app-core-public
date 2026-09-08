# Exemple de base : log sécurisé dans le terminal

Cet exemple crée un logger local, conserve un builder pour le composant de
l'application et écrit trois sévérités. La politique par défaut est Safe en V4.

```rust
use appcore_log::{LogOutputMode, LoggerConfig};

fn main() -> Result<(), appcore_log::LogConfigError> {
    // La configuration appartient à l'application et échoue explicitement.
    let logger = LoggerConfig {
        output: LogOutputMode::Terminal,
        ..LoggerConfig::default()
    }
    .build()?;

    // Réutiliser le builder évite de répéter le composant à chaque appel.
    let log = logger.dispatcher().event(0, "application");

    log.info("application démarrée");
    log.warn("connexion plus lente que prévu");
    log.error("document non enregistré");

    Ok(())
}
```

Sortie approximative :

```text
[Info][application][V4] application démarrée
[Warn][application][V4] connexion plus lente que prévu
[Error][application][V4] document non enregistré
```

Le timestamp `0` rend l'exemple déterministe ; une application réelle peut
utiliser `SystemLogClock` avec `event_now`. Aucun thread ou état global n'est créé.

Exécutez l'exemple maintenu dans le crate :

```shell
cargo run -p appcore-log --example basic
```

Étape suivante : [exemple intermédiaire](intermediate.fr.md).
