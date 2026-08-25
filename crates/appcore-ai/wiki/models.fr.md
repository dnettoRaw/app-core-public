# Modèles, configuration et training

[English](models.en.md) | [Português](models.pt.md) |
[Guide](guide.fr.md) | [LLM génératifs](generative-llm.fr.md) |
[Exemple Candle](examples/intermediate.fr.md) | [Recettes](recipes.fr.md)

Cette page sépare formats reconnus, backends exécutables et training réellement
implémenté. Dans `0.1.0-beta.2`, enregistrer la metadata d'un format ne signifie
pas qu'un engine sache l'inférer.

## Matrice réelle de support

| Format | Policy default | Backend inclus | Training inclus |
|---|---:|---:|---:|
| `NativeLinearV1` | accepté | Candle CPU | classification linéaire locale |
| GGUF | accepté | serveur llama.cpp/generic OpenAI-compatible | aucun |
| ONNX | accepté | serveur OpenVINO/generic OpenAI-compatible | aucun |
| SafeTensors | accepté | serveur MLX/vLLM/SGLang/TensorRT/Tabby/generic OpenAI-compatible | aucun |
| `Other(CapabilityId)` | refusé par défaut | adaptateur requis | adaptateur requis |

La crate livre `candle/cpu-linear-v1` et `OpenAiCompatibleBackend` opt-in. Ce
dernier dialogue avec un serveur déjà démarré ; il ne parse pas directement
GGUF/ONNX/SafeTensors et ne télécharge jamais silencieusement. Binding exact,
digest, device, format et capabilities restent obligatoires au deployment.

`ModelDescriptor::input_modalities` et `BackendDescriptor::input_modalities`
déclarent l'intersection réelle acceptée par une route. Le router refuse une
image envoyée à un adapter text-only même si son model ID a été mal enregistré.
`AiOptions::quality` filtre aussi automatiquement le `QualityTier` minimal. Un
model ID forcé ne contourne ni modalité, format, ressources ni privacy.

## Configurer un modèle génératif existant

Activez `backend-openai-compatible`, démarrez le moteur séparément en loopback
et associez `ModelId` au nom exact du serveur. L'exemple exige le vrai digest :

```bash
APPCORE_AI_ENGINE=llama.cpp \
APPCORE_AI_FORMAT=gguf \
APPCORE_AI_BASE_URL=http://127.0.0.1:8080 \
APPCORE_AI_MODEL=mon-modele \
APPCORE_AI_MODEL_SHA256=<digest-hexadecimal-64-caracteres> \
APPCORE_AI_MODEL_BYTES=<taille-exacte> \
cargo run -p appcore-ai --example openai_compatible \
  --features backend-openai-compatible
```

Profils : `llama.cpp`, `mlx-lm`, `vllm`, `sglang`, `tensorrt-llm`, `openvino`,
`tabbyapi`, `generic`. Tools, vision, seed et stop ne sont activés qu'après
preuve du tuple serveur/modèle. Le transport par défaut refuse les credentials ;
l'authentification distante exige un transport AppCore security.

## Configurer un NativeLinearV1 existant

Activez seulement l'inférence :

```toml
[dependencies]
appcore-ai = { version = "0.1.0-beta.2", default-features = false, features = ["backend-candle"] }
```

Construisez ou importez la matrice `[classes, input_dimensions]`, les biais et
les labels :

```rust
let dimensions = 256;
let labels = vec!["available".into(), "unavailable".into()];
let weights = vec![0.0_f32; labels.len() * dimensions];
let biases = vec![0.0_f32; labels.len()];
let artifact = NativeLinearArtifact::new(
    dimensions,
    labels,
    weights,
    biases,
)?;
let bytes = artifact.encode()?;
let identity = artifact.identity(None, false)?;
```

Ensuite :

1. écrivez les octets dans un `ArtifactStore` ;
2. créez un `ModelDescriptor` avec `ArtifactFormat::NativeLinearV1` ;
3. enregistrez `CandleBackend` dans `BackendRegistry` ;
4. enregistrez descriptor et localisation dans `ModelRegistry` ;
5. envoyez `AiTask::ClassifyText` via `AiRuntime`.

Le flux complet est dans
[`candle_runtime.rs`](../examples/candle_runtime.rs). Les poids sont `f32` ;
déclarez `Quantization::None`. Les autres valeurs de `Quantization` concernent
de futurs backends et ne quantifient pas automatiquement ce format.

## Entraîner une classification locale

Activez :

```toml
[dependencies]
appcore-ai = { version = "0.1.0-beta.2", default-features = false, features = ["training-candle"] }
```

Chaque exemple contient un texte non vide et l'index de classe :

```rust
let dataset: Arc<dyn TrainingDataset> = Arc::new(
    InMemoryTrainingDataset::new(
        vec![
            TrainingExample { text: "service ready".into(), label: 0 },
            TrainingExample { text: "healthy".into(), label: 0 },
            TrainingExample { text: "service failed".into(), label: 1 },
            TrainingExample { text: "unavailable".into(), label: 1 },
        ],
        1_000,
        512,
    )?,
);
```

Configurez toutes les bornes du job :

```rust
let job = TrainingJob {
    id: CapabilityId::new("job/service-status")?,
    model: ModelId::new("model/service-status")?,
    revision: "v1".into(),
    labels: vec!["available".into(), "unavailable".into()],
    input_dimensions: 256,
    epochs: 20,
    max_steps: 1_000,
    batch_size: 16,
    learning_rate: 0.1,
    seed: 42,
    resource_requirements: ResourceEstimate {
        cpu_percent: 60,
        memory_bytes: 32 * 1024 * 1024,
        workers: 1,
        ..ResourceEstimate::default()
    },
    resource_mode: AiResourceMode::Custom(AiResourceLimits {
        max_cpu_percent: 70,
        max_memory_bytes: 64 * 1024 * 1024,
        max_vram_bytes: 0,
        max_workers: 1,
        max_concurrent_jobs: 1,
    }),
    checkpoints: TrainingCheckpointPolicy {
        every_epochs: 5,
        max_checkpoints: 4,
    },
    resume: None,
    publisher: None,
    max_input_bytes: 512,
    max_output_bytes: 4 * 1024,
};
```

Exécutez et enregistrez le résultat :

```rust
let output = trainer
    .train(&job, dataset, progress, &cancellation)
    .await?;
models.register(output.descriptor.clone(), [ArtifactLocation::Memory])?;
```

Utilisez le même `ArtifactStore` pour trainer et backend : `CandleTrainer`
écrit déjà les artefacts finaux et checkpoints. Le programme reproductible est
[`candle_training.rs`](../examples/candle_training.rs) :

```bash
cargo run -p appcore-ai --example candle_training --features training-candle
```

## Signification des paramètres

| Champ | Effet |
|---|---|
| `labels` | ordre stable des classes ; le dataset utilise ces indexes |
| `input_dimensions` | largeur du vecteur hashé, pas nombre de tokens |
| `epochs` | maximum de passages complets sur le dataset |
| `max_steps` | plafond global pouvant arrêter avant le dernier epoch |
| `batch_size` | demande du job ; les modes prudents peuvent la réduire |
| `learning_rate` | taux SGD fini et positif |
| `seed` | initialisation reproductible des poids |
| `resource_requirements` | pic déclaré avant admission |
| `checkpoints` | fréquence et nombre maximum de snapshots |
| `resume` | identité exacte d'un `NativeLinearV1` compatible |

`Eco` utilise un batch effectif de 1. `Balanced` et `Custom` divisent le batch
demandé par deux en arrondissant vers le haut. `Performance` et `Unrestricted`
préservent la demande, toujours sous les plafonds du trainer.

## Limites default effectives

| Limite Candle trainer | Default |
|---|---:|
| exemples | 100 000 |
| dimensions | 4 096 |
| classes | 256, minimum 2 pour training |
| epochs | 100 |
| optimizer steps | 100 000 |
| batch | 512 |
| artefact encodé | 64 MiB |

Les labels font au maximum 96 octets. Chaque texte respecte les limites du
dataset et `job.max_input_bytes`. Le plafond effectif est toujours le minimum
entre policy, job, backend, artifact store et request.

## Choisir dimensions et données

`NativeLinearV1` utilise des features déterministes dérivées des octets du
texte. Il convient aux signaux lexicaux simples, routing, filtres et petites
classifications. Comme point initial, pas comme garantie de qualité :

| Problème | Dimensions initiales |
|---|---:|
| jusqu'à 20 labels, petit vocabulaire | 256–512 |
| 20–100 labels ou vocabulaire plus grand | 1 024–2 048 |
| jusqu'à 256 labels | 2 048–4 096, en mesurant collisions et RAM |

Séparez training et validation, équilibrez les exemples et mesurez precision,
recall et matrice de confusion. Plus d'epochs ne corrige ni mauvais labels, ni
classes ambiguës, ni données non représentatives.

## Resume, identité et provenance

Resume exige dimensions et labels identiques, dans le même ordre. Dans cette
beta, le trainer accepte une identité locale uniquement avec `signature_required = false`;
l'intégration du resume signé n'est pas encore connectée. Ne changez jamais les
octets en gardant un ID : SHA-256 et taille exacte sont l'identité réelle.

Pour l'activation signée en inférence, utilisez `ProvenanceArtifactStore` avec
un verifier de la sécurité AppCore. `ModelSecurityPolicy::default()` autorise
NativeLinearV1, GGUF, ONNX et SafeTensors, refuse les formats provider et borne
artefact/RAM/VRAM. Une deployment doit réduire ces maxima au hardware réel.

## Ceci n'est pas du training LLM

Le trainer actuel ne fait ni pretraining, fine-tuning, LoRA, génération,
embeddings, image, audio, GPU, ni training distribué. Entraînez ou affinez les
LLM hors Runtime, convertissez-les en format data-only et activez-les avec un
backend explicite. Voir le [profil LLM génératif](generative-llm.fr.md).
