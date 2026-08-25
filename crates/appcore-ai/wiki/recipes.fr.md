# Recettes concrètes appcore-ai

[English](recipes.en.md) | [Português](recipes.pt.md) |
[Guide](guide.fr.md) | [Exemple de base](examples/basic.fr.md) |
[Exemple intermédiaire](examples/intermediate.fr.md)

Cette page utilise les API réelles de `0.1.0-beta.2`. Elle ne suppose ni champ
V1 ni backend caché. La composition explicite existe via
`appcore-bin/ai-alpha` ; la sélection déclarative attend un contrat post-1.0.

## Choix rapide

| Besoin | Feature | Point de départ |
|---|---|---|
| normaliser ou classifier par règle | aucune | [`lightweight_runtime.rs`](../examples/lightweight_runtime.rs) |
| inférence linéaire CPU via runtime | `backend-candle` | [`candle_runtime.rs`](../examples/candle_runtime.rs) |
| appeler uniquement le SPI Candle | `backend-candle` | [`candle_cpu.rs`](../examples/candle_cpu.rs) |
| entraîner et écrire des checkpoints | `training-candle` | [`candle_training.rs`](../examples/candle_training.rs) |
| bridge vers les peers AppCore | `swarm` | `SwarmBridge` implémentée par l'hôte |
| texte, tools ou vision générative locale/privée | `backend-openai-compatible` | [`openai_compatible.rs`](../examples/openai_compatible.rs) |

## Budgets local et de contribution séparés

`AiContributionPolicy` n'augmente jamais le budget local. Cet exemple conserve
jusqu'à 70 % CPU et 64 MiB pour le travail local, mais annonce au maximum 25 %
CPU, 8 MiB RAM, deux workers et 512 MiB de stockage aux peers autorisés :

```rust
use appcore_ai::{
    AiContributionPolicy, AiResourceLimits, AiResourceMode, AiResult,
    ResourceGovernor, ResourceGovernorConfig, SystemHardwareProbe,
};

fn budgets() -> AiResult<()> {
    let contribution = AiContributionPolicy {
        contribute_compute: true,
        contribute_storage: true,
        max_cpu_percent: 25,
        max_gpu_percent: 0,
        max_memory_bytes: 8 * 1024 * 1024,
        max_vram_bytes: 0,
        max_storage_bytes: 512 * 1024 * 1024,
        max_workers: 2,
        max_concurrent_jobs: 1,
    };
    let governor = ResourceGovernor::new(
        SystemHardwareProbe::default(),
        ResourceGovernorConfig::default(),
        contribution,
    )?;
    let mode = AiResourceMode::Custom(AiResourceLimits {
        max_cpu_percent: 70,
        max_memory_bytes: 64 * 1024 * 1024,
        max_vram_bytes: 0,
        max_workers: 4,
        max_concurrent_jobs: 2,
    });
    let pair = governor.budgets(mode, 0)?;
    assert_eq!(pair.local.cpu_percent, 70);
    assert_eq!(pair.contribution.cpu_percent, 25);
    assert_eq!(pair.contribution.memory_bytes, Some(8 * 1024 * 1024));
    assert_eq!(pair.contribution.storage_bytes, 512 * 1024 * 1024);
    Ok(())
}
```

Pour un node strictement local, utilisez `AiContributionPolicy::default()`.
Les budgets donnés restent à zéro même si le mode local est `Performance` ou
`Unrestricted`.

## Cache local SHA-256 et activation atomique

`LocalArtifactCache` dérive le nom du digest ; un nom externe ne choisit jamais
le path final. Le store vérifie digest et taille avant activation :

```rust
use appcore_ai::{ArtifactDigest, ArtifactIdentity, LocalArtifactCache};

let bytes = b"bounded-model-bytes";
let identity = ArtifactIdentity {
    digest: ArtifactDigest::from_bytes(bytes),
    size_bytes: u64::try_from(bytes.len())?,
    publisher: None,
    signature_required: false,
};
let root = std::env::temp_dir().join(format!(
    "appcore-ai-cache-example-{}",
    std::process::id()
));
let cache = LocalArtifactCache::new(&root, 1024)?;
let path = cache.store(&identity, bytes)?;
assert_eq!(cache.load(&identity)?, bytes);
assert_eq!(path, cache.path(identity.digest));
std::fs::remove_dir_all(root)?;
```

Modifier `bytes` après la création de l'identité fait échouer `store` avec
`AiError::Integrity("artifact digest")`. Pour une signature obligatoire,
enveloppez un `ArtifactStore` dans `ProvenanceArtifactStore` ; son verifier
adapte la sécurité AppCore sans clé privée dans cette crate.

## Annulation coopérative et deadline

Le caller possède le token et peut annuler toutes ses copies. Le runtime le
vérifie avant routing, load et inférence ; les backends doivent coopérer :

```rust
let cancellation = appcore_ai::CancellationToken::new();
let mut request = appcore_ai::AiRequest::text(
    appcore_ai::AiTask::TransformText,
    "bounded input",
    limits,
)?;
request.options.execution = appcore_ai::AiExecutionMode::Local;
request.options.deadline = Some(std::time::Duration::from_millis(250));

cancellation.cancel();
let result = runtime
    .resolve_with_cancellation(request, cancellation)
    .await;
assert_eq!(result, Err(appcore_ai::AiError::Cancelled));
```

La deadline est relative au début de `resolve` ; elle ne tue pas un thread et
n'interrompt pas un backend bloquant. L'adaptateur doit fractionner le travail
long et consulter le token.

## Modes Local, Auto et Swarm sans ambiguïté

| Mode | Calcul distant | Stockage distant | Bridge obligatoire |
|---|---:|---:|---:|
| `Local` | jamais | seulement si explicite et pas `LocalOnly` | non |
| `Auto` | uniquement avec grant et policy | uniquement avec grant et policy | pour les candidats distants |
| `Swarm` | obligatoire | optionnel et indépendant | oui |

Une requête de calcul distant doit déclarer policy et grant :

```rust
use appcore_ai::{
    AiAuthorizationContext, AiExecutionMode, AiPrivacyMode, CapabilityId,
    REMOTE_COMPUTE_GRANT,
};

request.options.execution = AiExecutionMode::Swarm;
request.options.privacy = AiPrivacyMode::TrustedSwarm;
request.options.distribution.allow_remote_compute = true;
request.options.distribution.allow_remote_storage = false;
request.options.authorization = Some(AiAuthorizationContext {
    tenant: CapabilityId::new("tenant/example")?,
    subject: CapabilityId::new("subject/example")?,
    grants: vec![CapabilityId::new(REMOTE_COMPUTE_GRANT)?],
});
```

Sans `runtime.with_swarm_bridge(...)`, le résultat est
`AiError::SwarmUnavailable`. Le stockage distant exige aussi
`REMOTE_STORAGE_GRANT`. Combiner `LocalOnly` et une permission distante est une
entrée invalide. La bridge doit réutiliser authentification, discovery, replay
et Peer RPC AppCore.

## Training Candle local et reproductible

Exécutez le job complet :

```bash
cargo run -p appcore-ai --example candle_training --features training-candle
```

Sortie déterministe du dataset inclus :

```text
checkpoint epoch=2 step=4 loss=0.6634
checkpoint epoch=4 step=8 loss=0.3914
epochs=4 steps=8 final_loss=0.3914 artifact_bytes=2090 stored=true
```

Le programme configure explicitement dataset, seed, epochs, steps, batch,
ressources et fréquence des checkpoints. `TrainingOutput` contient les octets,
l'identité et un `ModelDescriptor` prêt à enregistrer :

```rust
let output = trainer
    .train(&job, dataset, progress, &cancellation)
    .await?;
models.register(
    output.descriptor.clone(),
    [appcore_ai::ArtifactLocation::Memory],
)?;
```

Utilisez le même `ArtifactStore` pour `CandleTrainer` et `CandleBackend` ; le
trainer a déjà stocké l'artefact final. Pour reprendre, assignez une identité
vérifiée à `job.resume`. Le training distribué n'est pas supporté.

## Observations expurgées et métriques

Connectez `AiObservationSink` à l'adaptateur `appcore-ops` de la composition.
Les événements ne contiennent ni prompt, output, model ID, peer ID, ni credential :

```rust
use appcore_ai::{AiObservation, AiObservationSink};

struct OpsAdapter;

impl AiObservationSink for OpsAdapter {
    fn record(&self, observation: &AiObservation) {
        match observation {
            AiObservation::RequestCompleted { success, attempts, .. } => {
                record_counter("ai.request.completed", *success, *attempts);
            }
            _ => record_event_class(observation),
        }
    }
}

let runtime = runtime.with_observation_sink(std::sync::Arc::new(OpsAdapter));
```

`record_counter` et `record_event_class` sont des fonctions de l'adaptateur
hôte, pas des API de la crate. Pour le polling local, utilisez
`runtime.telemetry()` et publiez seulement les champs agrégés.

## Backpressure avant le backend

Utilisez une `FairQueue` par domaine de dispatch et refusez l'overload de façon
structurée :

```rust
use appcore_ai::{
    AiPriority, CancellationToken, FairQueue, FairQueueConfig, QueueAdmission,
};
use std::time::Duration;

let mut queue = FairQueue::new(FairQueueConfig {
    capacity: 2,
    starvation_after: Duration::from_secs(1),
    overload_retry_after: Duration::from_millis(25),
})?;
assert!(matches!(
    queue.enqueue("one", AiPriority::Normal, 0, None, CancellationToken::new()),
    QueueAdmission::Queued { .. }
));
queue.enqueue("two", AiPriority::High, 0, None, CancellationToken::new());
let third = queue.enqueue(
    "three",
    AiPriority::Normal,
    0,
    None,
    CancellationToken::new(),
);
assert!(matches!(third, QueueAdmission::Rejected { .. }));
```

Partitionnez `DynamicBatcher` avec la `BatchKey` complète : modèle, backend,
device et classe de task doivent correspondre. Ne regroupez jamais des requêtes
uniquement parce que leur input a le même type.

## Diagnostic rapide

| Erreur | Première vérification |
|---|---|
| `NotFound("compatible AI route")` | task, model ID, état, localisation, backend et device |
| `Capacity("all model routes were denied")` | `ResourceEstimate`, mode, RAM/VRAM connue et pression |
| `Unauthorized` | tenant, grants compute/storage séparés et privacy |
| `SwarmUnavailable` | feature `swarm`, bridge composée et annonces vivantes |
| `Integrity` | digest, taille, publisher, signature et validité |
| `BackendUnavailable` | lifecycle du modèle et santé du backend |
| `LimitExceeded` | limite nommée, taille réelle et attempts/peers/batch |

Les erreurs font partie du contrat. N'ajoutez pas de fallback silencieux,
n'activez pas automatiquement `Unrestricted` et ne transformez pas une capacité
inconnue en capacité infinie.
