# appcore-log

[English](README.en.md) | [Português](README.pt.md)

`appcore-log` fournit des logs opérationnels structurés, bornés et sécurisés
pour AppCore. Il n'utilise ni logger global ni file cachée, et ne crée aucun
worker en arrière-plan sans sélection explicite de `AsyncSink`. L'application
choisit filtres, destinations, rétention et durabilité.

## Démarrage rapide

```rust
use appcore_log::{LogOutputMode, LoggerConfig};

fn main() -> Result<(), appcore_log::LogConfigError> {
    let logger = LoggerConfig {
        output: LogOutputMode::Terminal,
        ..LoggerConfig::default()
    }
    .build()?;

    let log = logger.dispatcher().event(0, "application");

    log.info("application démarrée");
    log.warn("connexion lente");

    Ok(())
}
```

Une application `appcore-sdk` configure le même logger avec `App::logging` et
écrit avec `app.log(...)` ou `app.logger()`.

## Modes de sortie

| Mode | Exécution normale | Fichier requis |
|---|---|---|
| `Terminal` | Texte lisible dans le terminal | Non |
| `File` | JSONL structuré et borné | Oui |
| `TerminalAndFile` | Terminal et JSONL | Oui |
| `Disabled` | Aucun sink ni conversion du message | Non |
| `CrashOnly` | Anneau mémoire assaini et borné | Oui, créé seulement par `dump_crash` |

`CrashOnly` n'installe pas de panic handler. Appelez `dump_crash` une fois à la
frontière de crash contrôlée par l'application.

## Fichiers bornés

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

    logger.dispatcher().event(0, "application").info("prête");

    Ok(())
}
```

Le dossier actif contient `runtime.jsonl`, `runtime.jsonl.1` et
`runtime.jsonl.2`. Les rotations plus anciennes vont dans `archive/AAAA/MM`
jusqu'à la limite globale `max_files`. L'application choisit le nom du fichier.
Créez le dossier parent du fichier actif pendant le setup ; les dossiers année
et mois de l'archive sont créés à la rotation. Les destinations fichier et
archive refusent les liens symboliques au lieu de les suivre.

Utilisez `sync_each_write: true` si chaque événement doit atteindre le stockage
avant le retour. `false` évite ce syscall et favorise le débit. Les constantes
vont de `LOG_SIZE_1_MIB` à `LOG_SIZE_64_MIB`; tout `u64` non nul reste valide.

## Sévérité, verbosité et composants

La sévérité représente l'impact, de `Trace` à `Critical`. La verbosité
représente le détail, de V1 à V9. Une politique V4 accepte V1 à V4 ; elle ne
signifie pas « sévérité quatre ». Les surcharges sont hiérarchiques : `sync`
couvre aussi `sync.transport`, sauf configuration plus précise.

## Frontière de sécurité

Les politiques Safe et Diagnostic masquent les secrets typés et remplacent les
paths typés par des alias avant les sinks ordinaires. Utilisez
`LogEvent::secret` et `LogEvent::path` ; ne mettez jamais d'identifiant secret
dans le message libre. Les paths complets demandent une décision explicite.

Les diagnostics Sensitive exigent `Sensitivity::Sensitive` et un
`SensitiveDntSink` explicite. Le contenu est authentifié et chiffré en DNT,
sans fallback terminal, JSONL ou mémoire ordinaire. Les champs secrets restent
masqués même dans ce mode.

## Limites et échecs

- Texte : 4 Kio par champ textuel ou d'identité.
- Fields structurés : 32 par événement.
- Clés : 128 octets ; valeurs : 4 Kio.
- Rotations actives : 32 au maximum.
- Fichiers archivés : de 1 à 10 000.
- Anneaux : bornés par nombre et mémoire estimée.
- Échecs de sink : comptés sans logging récursif.

Utilisez `stats()` pour les compteurs globaux et `sink_stats()` par destination.
Utilisez `FixedLogClock` pour les tests déterministes.

Consultez le [guide français](wiki/guide.fr.md) et les exemples exécutables dans
[`examples/`](examples/).

Avant d'allouer le payload JSONL, le sink compte les octets sérialisés, avec
échappements et fin de ligne, selon `max_bytes`. Les enregistrements trop grands
retournent `Capacity` avant rotation ou écriture. Les autres utilisent une
réservation de taille exacte et une seconde sérialisation ; le surcoût de
l'allocateur, les événements de l'appelant et le filesystem sont hors budget.

Chaque `FileSink` conserve un seul descripteur append ouvert et suit la taille
du fichier actif sous le mutex existant. Il ferme ce descripteur avant rotation
et le rouvre si le chemin est supprimé, remplacé, tronqué ou modifié par un
autre writer. Il n'existe toujours ni file cachée ni worker de flush en arrière-plan.

Pour une frontière asynchrone explicite, enveloppez un sink dans `AsyncSink`.
Son `emit` non bloquant est borné par `AsyncSinkConfig::max_events` et
`max_bytes` ; la saturation retourne `Capacity` et est comptée. `flush` attend
les événements admis et `shutdown` draine puis joint l'unique worker de 256 Kio.
Le sink interne doit terminer : un I/O bloquant arbitraire ne peut pas être
forcé à s'arrêter sûrement. Un drop sans shutdown explicite n'attend jamais cet
I/O. La file est opt-in et ne modifie pas les defaults de `LoggerConfig`.

## Benchmark

Exécutez les dix workloads avec le contexte matériel :

```shell
cargo run -p appcore-dev -- bench --name appcore-log \
  --output target/appcore-log-benchmark.json
```

Le rapport contient distributions temporelles, temps CPU, RSS maximal/conservé
et contexte CPU, RAM, GPU, système et filesystem. Comparez les rapports
compatibles avec `appcore-dev bench compare`.

Les cas JSONL concurrents mesurent 64 événements par lot de quatre producteurs,
avec et sans sync par événement. Le sink lent sérialise quatre événements avec
une attente simulée de 1 ms chacun ; il ne mesure pas un disque physique.
Consultez le guide pour les limites de mesure.

## Documentation stable

Identifiant stable : **ACR-027**. Consultez le
[guide complémentaire d’architecture et d’intégration](https://wiki.appcore.dnettoraw.com/fr/crates/id/acr-027). Cet identifiant
permanent reste valable si la page du wiki est déplacée.
