# Runtime lightweight complet sans framework ML

[English](basic.en.md) | [Português](basic.pt.md) |
[Exemple intermédiaire](intermediate.fr.md) | [Recettes](../recipes.fr.md) |
[Guide](../guide.fr.md)

Cet exemple compose un vrai `AiRuntime`, enregistre une règle déterministe,
exécute une classification locale et lit le diagnostic et la télémétrie. La
compilation par défaut suffit : aucune dépendance Candle, aucun téléchargement
de modèle et aucun accès réseau.

## Exécuter

Depuis le workspace AppCore :

```bash
cargo run -p appcore-ai --example lightweight_runtime
```

Sortie :

```text
label=operational score=1.000
route=Lightweight attempts=1
requests=1 successes=1 lightweight=1
```

Le source compilé se trouve dans
[`examples/lightweight_runtime.rs`](../../examples/lightweight_runtime.rs).

## Dépendance

Dans un consommateur indépendant :

```toml
[dependencies]
appcore-ai = { version = "0.1.0-beta.3", default-features = false }
```

`appcore-ai` utilise un SemVer indépendant. Épinglez délibérément la version
beta et relisez les changements avant une mise à jour.

## Composition minimale

```rust
use appcore_ai::{
    AiContributionPolicy, AiExecutionMode, AiLimits, AiPrivacyMode, AiRequest,
    AiResponse, AiResult, AiRuntime, AiTask, BackendRegistry, GovernorAdmission,
    LightweightEngine, ModelRegistry, ResourceGovernor, ResourceGovernorConfig,
    RuleMatch, SystemAiClock, SystemHardwareProbe, TextRule,
};
use std::sync::Arc;

fn build_runtime(limits: AiLimits) -> AiResult<AiRuntime> {
    let lightweight = LightweightEngine::new(
        vec![TextRule {
            label: "service.status".into(),
            pattern: "status".into(),
            output: "operational".into(),
            matching: RuleMatch::Exact,
        }],
        limits,
        8_000,
    )?;
    let governor = ResourceGovernor::new(
        SystemHardwareProbe::default(),
        ResourceGovernorConfig::default(),
        AiContributionPolicy::default(),
    )?;
    let admission = GovernorAdmission::new(governor, SystemAiClock::new());
    AiRuntime::new(
        limits,
        Arc::new(lightweight),
        Arc::new(ModelRegistry::new()),
        Arc::new(BackendRegistry::new()),
        Arc::new(admission),
    )
}

async fn classify(runtime: &AiRuntime, limits: AiLimits) -> AiResult<AiResponse> {
    let mut request = AiRequest::text(AiTask::ClassifyText, "status", limits)?;
    request.options.execution = AiExecutionMode::Local;
    request.options.privacy = AiPrivacyMode::LocalOnly;
    request.options.include_diagnostics = true;
    runtime.resolve(request).await
}
```

Même sans modèle, `ModelRegistry`, `BackendRegistry` et `ModelAdmission` sont
obligatoires. La composition reste ainsi explicite et l'hôte peut ajouter un
backend sans modifier le contrat de requête.

## Ce que protège chaque borne

L'exemple exécutable réduit `max_input_bytes` et `max_output_bytes` à 256.
`AiLimits` borne aussi les parties d'entrée, les métadonnées et les tentatives.
Utilisez la même valeur pour l'entrée, l'engine et le runtime.

Une entrée dépassant le plafond échoue avant le resolver :

```rust
let limits = AiLimits {
    max_input_bytes: 4,
    ..AiLimits::default()
};
let error = AiRequest::text(AiTask::TransformText, "cinq!", limits)
    .expect_err("input must exceed the four-byte limit");
assert!(matches!(error, appcore_ai::AiError::LimitExceeded { .. }));
```

## Autre opération sans modèle

`TransformText` normalise les espaces sans consulter les règles :

```rust
let mut request = AiRequest::text(
    AiTask::TransformText,
    "  texte\t  local\nborné  ",
    limits,
)?;
request.options.execution = AiExecutionMode::Local;
request.options.privacy = AiPrivacyMode::LocalOnly;
let response = runtime.resolve(request).await?;
assert_eq!(response.output, appcore_ai::AiOutput::Text("texte local borné".into()));
```

## Garanties observables

- `LocalOnly` et `Local` excluent calcul et stockage distants.
- `include_diagnostics` retourne routes et tentatives, jamais le prompt.
- `Debug` des requêtes/réponses n'affiche que des tailles expurgées.
- sans règle ni modèle compatible, l'erreur est
  `AiError::NotFound("compatible AI route")`.
- l'exemple ne contribue aucune ressource au Swarm car
  `AiContributionPolicy::default()` donne zéro calcul et zéro stockage.

Continuez avec l'[exemple intermédiaire](intermediate.fr.md) pour charger un
artefact vérifié et exécuter Candle via le même `AiRuntime`.
