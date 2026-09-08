# Guide AppCore Log

Ce guide va du terminal à la rotation, au mode crash et aux diagnostics
chiffrés. Le logger appartient à son propriétaire : aucune configuration
globale, thread ou file n'est créée implicitement.

## 1. Choisir une destination

`LoggerConfig::default()` sélectionne le terminal sécurisé en V4.

| `LogOutputMode` | Comportement |
|---|---|
| `Terminal` | Écrit un texte lisible sur stdout. |
| `File` | Écrit un objet JSON par ligne. |
| `TerminalAndFile` | Envoie l'événement assaini aux deux destinations. |
| `Disabled` | Retourne avant conversion du message ou appel d'un sink. |
| `CrashOnly` | Conserve un anneau borné et ne crée normalement aucun fichier. |

Les modes fichier exigent `file: Some(...)`. Une erreur renvoie
`LogConfigError`, sans fallback silencieux.

```rust
use appcore_log::{LogOutputMode, LoggerConfig};

let logger = LoggerConfig {
    output: LogOutputMode::Terminal,
    ..LoggerConfig::default()
}
.build()
.expect("configuration valide");
```

## 2. Réutiliser un composant

```rust
use appcore_log::LoggerConfig;

let logger = LoggerConfig::default()
    .build()
    .expect("configuration valide");
let log = logger.dispatcher().event(0, "sync");

log.info("réplication démarrée");
log.warn("peer temporairement indisponible");
log.error("checkpoint non persisté");
```

Une application SDK configure une fois avec `App::logging(config)` et emploie
`app.logger()`.

## 3. Borner fichiers et rétention

| Champ `FileSinkConfig` | Rôle |
|---|---|
| `path` | Dossier actif et nom du fichier. |
| `max_bytes` | Taille maximale non nulle de chaque JSONL. |
| `sync_each_write` | Flush durable de chaque événement si vrai. |
| `retention` | Rotations proches du fichier actif, de 0 à 32. |
| `archive` | Destination optionnelle des rotations excédentaires. |

`FileArchiveConfig.directory` reçoit les dossiers `AAAA/MM`. `max_files` borne
l'archive entière entre 1 et 10 000 fichiers.

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
.expect("configuration valide");
```

Utilisez `LOG_SIZE_1_MIB` à `LOG_SIZE_64_MIB`, ou tout `u64` non nul. La valeur
false de `sync_each_write` favorise le débit ; true favorise la durabilité de
chaque événement au prix d'I/O supplémentaires.

L'application doit créer le dossier parent du fichier actif avant le premier
événement. Le sink crée les dossiers d'archive `AAAA/MM` si nécessaire et
refuse les destinations par lien symbolique.

La sérialisation JSONL compte les octets sans allouer le payload avant de
vérifier `max_bytes`, échappements et fin de ligne compris. Les enregistrements
rejetés ne déclenchent ni rotation ni modification. Les autres réservent la
taille exacte puis sont sérialisés à nouveau : une seconde passe borne
l'allocation temporaire. Ce n'est pas un plafond RSS ; surcoût de l'allocateur
et événements de l'appelant restent séparés.

## 4. Séparer sévérité et verbosité

La sévérité est l'impact : `Trace`, `Debug`, `Info`, `Warn`, `Error` ou
`Critical`. La verbosité est le détail requis : V1–V3 pour les échecs
importants, V4–V6 pour l'exploitation et V7–V9 pour le diagnostic. Une politique
V4 accepte V1–V4.

| Plage | Usage typique |
|---|---|
| V1–V3 | Échecs critiques, erreurs importantes et warnings pertinents. |
| V4–V6 | Événements essentiels, exploitation normale et flux. |
| V7–V9 | Debug technique, I/O, timing et diagnostic profond. |

```rust
use appcore_log::{LogPolicy, Verbosity};

let mut policy = LogPolicy::new(Verbosity::V4);
policy.set_component("sync", Verbosity::V8);
```

La surcharge `sync` couvre aussi `sync.transport` ; la plus précise gagne.
`log.verbosity(7).debug(...)` ne modifie que cet événement.

## 5. Protéger secrets et paths

Utilisez `LogEvent::field` pour une donnée ordinaire, `LogEvent::path` pour un
path local et `LogEvent::secret` pour un identifiant secret. Safe et Diagnostic
assainissent avant terminal, fichier ou anneau. `PathAliases` produit des alias
comme `<APP_ROOT>` ; un path inconnu devient `<LOCAL_PATH>`.

```rust
use appcore_log::{LogEvent, Severity, Verbosity};

let event = LogEvent::new(0, Severity::Info, Verbosity::V4, "storage", "ouvert")
    .path("file", "/srv/app/data.json")
    .secret("token", "ne doit pas apparaître");
```

N'insérez pas de secrets ou de paths libres dans le message. Les champs typés
sont la frontière contrôlable. Le mode Sensitive exige un `SensitiveDntSink`
explicite et un DNT chiffré ; le dispatcher refuse les sinks ordinaires et ne
fait jamais de fallback plaintext.

## 6. Capturer uniquement un crash

Le mode crash borne la mémoire avec `crash_events` et `crash_bytes`. Il
n'installe pas de panic handler. Appelez `dump_crash` une fois depuis la
frontière de crash contrôlée ; le fichier reste absent avant cet appel.

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
.expect("configuration valide");

// Exécuter une fois depuis la frontière de crash contrôlée.
let _written = logger.dump_crash().expect("dump de crash écrit");
```

## 7. Observer les échecs et tester

- `stats()` expose événements filtrés, invalides, échoués et abandonnés.
- `sink_stats()` sépare les échecs par destination.
- `FixedLogClock` fournit des timestamps déterministes.
- Les limites sont 4 Kio par texte/identité, 32 fields, 128 octets par clé et
  4 Kio par valeur.

Un échec de sink est compté sans logging récursif. Tout anneau exige une limite
en nombre et en octets.

Continuez avec l'[exemple de base](examples/basic.fr.md) ou
l'[exemple intermédiaire](examples/intermediate.fr.md).

## 8. Mesurer sur le matériel réel

Le benchmark natif couvre émission désactivée/filtrée, assainissement, anneau
borné, JSONL buffered/durable et rotation vers l'archive :

```shell
cargo run -p appcore-dev -- bench --name appcore-log \
  --output target/appcore-log-benchmark.json
```

Le JSON contient p50/p95, CPU, RSS maximal/conservé, toolchain et matériel.
Conservez un baseline et utilisez `appcore-dev bench compare` avant d'accepter
une optimisation.

Le sink fichier synchrone réutilise un seul descripteur append. Son mutex couvre
le comptage, la détection de remplacement, la rotation, l'écriture et le sync
optionnel. Il rouvre après suppression, remplacement, troncature ou append
externe ; il ne crée pas de file asynchrone et ne change pas la durabilité choisie.

`AsyncSink` est l'alternative asynchrone explicite. Configurez les plafonds du
nombre d'événements et des octets estimés retenus, fournissez-le comme
`LogSink`, puis appelez `flush` ou `shutdown` à la frontière de lifecycle
propriétaire. L'admission n'attend jamais et retourne `Capacity` à saturation.
Le worker isole les panics du sink, expose les compteurs et utilise une pile de
256 Kio. L'arrêt n'est borné que si l'I/O interne l'est aussi ; le drop n'attend pas.

Les cas `jsonl_concurrent_buffered_batch_64` et
`jsonl_concurrent_synced_batch_64` utilisent quatre producteurs partageant un
dispatcher et un sink fichier, avec 16 événements chacun par itération. Le
temps inclut création des événements, assainissement, I/O et deux rendez-vous
par barrière par lot ; création/join des threads et préparation/suppression du
répertoire sont exclus. `serialized_slow_sink_batch_4` émet un événement par
producteur dans un sink sérialisé par mutex avec une attente de 1 ms par
événement. Il simule le blocage, pas la latence d'un disque physique. Les temps
sont par lot, pas par événement ni des percentiles de latence des producteurs.
Le RSS inclut les piles des threads. Ces cas mesurent la baseline synchrone ;
les tests unitaires, plutôt que le timing, prouvent les limites pendant la
livraison active et le flush/shutdown explicite. Ils ne prouvent pas la
durabilité après crash.
