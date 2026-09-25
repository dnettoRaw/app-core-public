# appcore-sdk

Le benchmark `runtime` mesure construction par défaut, préparation vide et
préparation de 32 commandes. Lancez `cargo bench -p appcore-sdk --bench runtime`.
`APPCORE_BENCH_CASE` sélectionne `construct_defaults`, `prepare_empty` ou
`prepare_32_commands`. Il mesure construction/validation/destruction, pas le
démarrage HTTP ni le cluster. `--features full` change le profil compilé sans
exercer toutes les capabilities optionnelles.

[English](README.en.md) | [Português](README.pt.md)

`appcore-sdk` est la façade documentée pour les applications AppCore. Elle
préserve une petite dépendance initiale et laisse l'infrastructure aux crates
qui en sont propriétaires. Elle remplace `appcore-bin`, désormais retiré, sans
conserver l'ancien host ni la CLI Runtime.

```rust
use appcore_sdk::prelude::*;

fn main() -> AppResult<()> {
    appcore_sdk::run("hello-world", |app| {
        app.log("Bonjour, monde !");
        Ok(())
    })
}
```

Implémentez `Application` pour les commandes, queries et tâches du déploiement
explicite. La façade de base ne requiert que les contrats, le Runtime core et
le logging borné. `App::prepare` exécute tous les hooks d'enregistrement et
retourne les registres Core immuables, un routeur de queries figé et les tâches
bornées sans démarrer un host.
Activez `api` pour les queries HTTP, `scheduler` pour le travail planifié,
`deployment` pour les bindings résolus par provider, ou `storage`, `sync`,
`ai` et `filemaker` seulement lorsque ces capabilities sont nécessaires.
Providers, listeners et cycle du processus restent des responsabilités de
l'application/deployment.

## Exemples exécutables

Les exemples sont organisés par progression dans `examples/` : `01-basics`,
`02-manifests`, `03-application`, `04-storage`, `05-sync`, `06-ai` et
`07-filemaker`, `08-logging` et `09-api`. Exécutez
`cargo run -p appcore-sdk --example NOM`; les cas de
capability exigent la feature correspondante.

Utilisez `App::logging(LoggerConfig)` pour sélectionner le terminal, un JSONL
borné, les deux, aucun log ou seulement les crashs. Le nom, les octets par
fichier, les rotations actives et l'archive optionnelle `YYYY/MM` sont
explicites. Le mode crash s'écrit avec `App::dump_crash_log` et ne crée aucun
fichier pendant l'exécution normale. Consultez `08-configured-logging`.

## Documentation stable

Identifiant stable : **ACR-028**. Consultez le
[guide complémentaire d’architecture et d’intégration](https://wiki.appcore.dnettoraw.com/fr/crates/id/acr-028). Cet identifiant
permanent reste valable si la page du wiki est déplacée.

Les applications existantes doivent suivre le
[guide de migration](wiki/migration.fr.md) maintenu par le crate.

`DiagnosticBundleBuilder` crée un `DiagnosticBundleV1` borné pour le support.
Il contient la plateforme, l’environnement, les versions, une empreinte de
stockage non réversible, les états Gateway/sync/update et les erreurs récentes
sûres. Le texte est borné et expurgé avant export; secrets et chemins bruts
sont exclus par défaut. `to_json` utilise le marqueur stable
`appcore.sdk.diagnostic.v1`; l’opt-in sensible est déclaré sans collecter de
valeurs secrètes.

`EnvironmentProfile` standardise dev/QA/production, local/release,
desktop/sync-node/mobile, standalone/cluster, les labels optionnels de
cluster/tenant, le namespace de stockage et le canal de mise à jour. Il refuse
les combinaisons incohérentes et fournit `diagnostic_label()` sans exposer de
secrets ni de chemins. Utilisez `validate_namespace_separation` avant une
promotion entre namespaces non-release et release.

Utilisez `CapabilityRegistry` pour déclarer les capabilities applicatives et
lier les commandes. `coverage` signale les commandes sans liaison et impose une
inscription bornée. `CapabilityOutcome` uniformise les résultats
`authentication_required` et `permission_denied`; le SDK n’évalue pas
l’autorisation métier et n’accorde aucune permission.
