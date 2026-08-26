# Inférence Candle locale via AiRuntime

[English](intermediate.en.md) | [Português](intermediate.pt.md) |
[Exemple de base](basic.fr.md) | [Recettes](../recipes.fr.md) |
[Guide](../guide.fr.md)

Cet exemple couvre le flux complet : créer un artefact data-only, vérifier et
stocker ses octets, enregistrer modèle et backend, appliquer l'admission de
ressources, charger à la demande puis classifier sur CPU via `AiRuntime`.

## Exécuter

```bash
cargo run -p appcore-ai --example candle_runtime --features backend-candle
```

Sortie :

```text
class=class-a score=1.000
route=Local { backend: BackendId("candle/cpu-linear-v1"), device: DeviceId("local/cpu/candle") }
model_state=Ready
loads=1 local_placements=1 successes=1
```

Le programme complet compilé est
[`examples/candle_runtime.rs`](../../examples/candle_runtime.rs). L'exemple
[`candle_cpu.rs`](../../examples/candle_cpu.rs) appelle directement le SPI du
backend ; une application doit préférer le flux `AiRuntime` de cette page.

## Dépendance et feature

```toml
[dependencies]
appcore-ai = { version = "0.1.0-beta.3", default-features = false, features = ["backend-candle"] }
```

Le build par défaut reste sans Candle. La feature ne télécharge aucun modèle
et ne supporte que le format CPU borné `NativeLinearV1`.

## 1. Dériver l'identité des octets

L'artefact contient 256 features déterministes, deux classes, poids et biais.
Il ne contient que des données, aucun code ni custom op.

```rust
let dimensions = 256;
let mut weights = vec![0.0; dimensions * 2];
weights[usize::from(b'a')] = 10.0;
weights[dimensions + usize::from(b'b')] = 10.0;
let artifact = NativeLinearArtifact::new(
    dimensions,
    vec!["class-a".into(), "class-b".into()],
    weights,
    vec![0.0, 0.0],
)?;
let bytes = artifact.encode()?;
let identity = artifact.identity(None, false)?;
```

`identity` fixe SHA-256 et la taille exacte. Modifier un octet fait retourner
`AiError::Integrity` par `store` ou `load`. En production, utilisez un
`publisher` et `signature_required = true` avec `ProvenanceArtifactStore` si la
politique impose une signature.

## 2. Stocker les octets et décrire le modèle

```rust
let memory = Arc::new(MemoryArtifactStore::new(4 * 1024 * 1024)?);
memory.store(&identity, &bytes, &CancellationToken::new())?;
let store: Arc<dyn ArtifactStore> = memory;

let descriptor = ModelDescriptor {
    id: ModelId::new("example/candle-runtime")?,
    revision: "v1".into(),
    tasks: vec![AiTask::ClassifyText],
    input_modalities: vec![AiModality::Text],
    format: ArtifactFormat::NativeLinearV1,
    quantization: Quantization::None,
    estimated_memory_bytes: u64::try_from(bytes.len())?.saturating_mul(2),
    estimated_vram_bytes: 0,
    max_input_bytes: 1_024,
    max_output_bytes: 1_024,
    context_limit: None,
    supported_backends: vec![BackendId::new(CANDLE_LINEAR_BACKEND_ID)?],
    supported_devices: vec![DeviceKind::Cpu],
    load_cost_units: 20,
    quality: Some(QualityTier::Tiny),
    artifact: identity,
};
```

Les plafonds du descriptor participent à l'admission. Ne déclarez pas de
valeurs inférieures au vrai pic du backend.

## 3. Enregistrer sans découverte implicite

```rust
let backends = Arc::new(BackendRegistry::new());
backends.register(Arc::new(CandleBackend::new(
    store,
    CandleBackendConfig::default(),
)?))?;

let models = Arc::new(ModelRegistry::new());
models.register(descriptor, [ArtifactLocation::Memory])?;
```

L'état initial est `Available` : les octets existent mais les tensors ne sont
pas chargés. Le premier `resolve` effectue `Available -> Loading -> Ready`.
Un doublon échoue ; aucun remplacement silencieux n'existe.

## 4. Admission avec capacité explicite

`SystemHardwareProbe::default()` lit la capacité CPU/RAM réelle sous macOS,
Linux et Windows et conserve les métriques d'accélérateur indisponibles à
`None`. L'exemple utilise `Custom` pour imposer une limite inférieure à celle
détectée sur l'hôte :

```rust
request.options.resources = AiResourceMode::Custom(AiResourceLimits {
    max_cpu_percent: 80,
    max_memory_bytes: 16 * 1024 * 1024,
    max_vram_bytes: 0,
    max_workers: 1,
    max_concurrent_jobs: 1,
});
```

Le backend estime 75 % CPU, un worker, zéro VRAM et une RAM égale au maximum
entre le descriptor et deux fois l'artefact. Un plafond inférieur refuse la
route avec `AiError::Capacity("all model routes were denied")`.

## 5. Forcer modèle, confidentialité et diagnostic

```rust
let mut request = AiRequest::text(AiTask::ClassifyText, "a", limits)?;
request.options.execution = AiExecutionMode::Local;
request.options.privacy = AiPrivacyMode::LocalOnly;
request.options.model = Some(ModelId::new("example/candle-runtime")?);
request.options.resources = custom_limits;
request.options.include_diagnostics = true;

let response = runtime.resolve(request).await?;
```

Forcer l'ID empêche la sélection d'un autre modèle compatible. `LocalOnly`
exclut calcul et stockage distants. `include_diagnostics` expose les tentatives
backend/device bornées, sans copier input, output ni credentials.

## État et télémétrie après l'appel

```rust
assert_eq!(models.get(&model_id)?.state, ModelState::Ready);
let metrics = runtime.telemetry();
assert_eq!(metrics.requests, 1);
assert_eq!(metrics.model_load_successes, 1);
assert_eq!(metrics.local_placements, 1);
assert_eq!(metrics.successes, 1);
```

Un second appel réutilise le modèle `Ready` ; `model_load_successes` reste à 1.
Les percentiles utilisent des buckets fixes approximatifs et les métriques
n'emploient jamais les IDs de modèle, tenant, peer ou prompt comme labels.

## Échecs explicites

| Situation | Résultat |
|---|---|
| feature `backend-candle` absente | backend absent de l'API compilée |
| digest ou taille divergents | `AiError::Integrity` |
| modèle sans localisation | aucune route locale compatible |
| format/task/device incompatible | route exclue avant inférence |
| RAM ou CPU insuffisants | candidat refusé par l'admission |
| token annulé avant le load | `AiError::Cancelled` |
| deadline écoulée | `AiError::DeadlineExceeded` |
| backend dupliqué | `AiError::Conflict("backend id")` |

Pour checkpoints, reprise et enregistrement d'un descriptor entraîné, suivez
la [recette training local](../recipes.fr.md#training-candle-local-et-reproductible).
