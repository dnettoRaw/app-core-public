# Runtime adaptatif pour LLM et IA multimodale

[English](generative-llm.en.md) | [Português](generative-llm.pt.md) |
[Guide](guide.fr.md) | [Modèles et training](models.fr.md) |
[Architecture](architecture-adr.fr.md) | [Threat model](threat-model.fr.md)

> État : `backend-openai-compatible` livre un transport borné texte/chat/tools
> et vision opt-in pour des serveurs configurés explicitement. Candle reste le
> backend classificateur data-only. `AnalyzeDocument` exige encore un backend
> document ; aucun parseur PDF/OCR universel n'est embarqué.

La cible n'est ni un engine universel ni une dépendance obligatoire envers un
autre runtime. `appcore-ai` est le plan de contrôle qui choisit la meilleure
route installée selon la requête, le matériel et la policy. Chaque engine reste
isolé derrière `InferenceBackend` et peut être remplacé sans changer
l'application.

Colibri n'est ni une dépendance ni un profil d'engine. AppCore possède un plan
VRAM/RAM/stockage vérifiable, un manifest de segments et un lecteur de ranges
vérifiés. Le streaming d'experts n'est annoncé que par un backend qui consomme
réellement ces segments.

Les recommandations ont été révisées le 2026-08-21. Il faut fixer la version
de l'engine, le digest, le format, le tokenizer, le chat template et la licence
dans chaque deployment.

## Sens de « couteau suisse »

```text
AiRequest
  task       -> générer texte | analyser image | analyser document | décider | embed
  input      -> Text | Image | Document | Audio | Video | Opaque
  quality    -> Fast | Balanced | Deep | Maximum
  latency    -> Interactive | Balanced | Throughput | Background
  placement  -> Local | Swarm | Auto
        |
        v
validation et privacy
  -> chemin déterministe lorsqu'il suffit
  -> modèles et adapters compatibles avec toutes les modalités
  -> admission RAM/VRAM/CPU/deadline
  -> score load, queue, latence, throughput, residency et coût
  -> engine persistant sélectionné
  -> réponse bornée et diagnostic expurgé
```

Supporter plusieurs engines signifie accepter des adapters conformes, et non
compiler tous les frameworks dans le core. Le build par défaut reste léger et
un deployment n'active que les adapters nécessaires.

## Vitesse contre profondeur

`AiOptions::quality` impose un minimum explicite :

| Profil | `QualityTier` minimal | Usage typique |
|---|---|---|
| `Fast` | `Tiny` | UI, autocomplétion, classification, réponse courte |
| `Balanced` | `Small` | assistant local général |
| `Deep` | `Balanced` | documents, code et analyse difficile |
| `Maximum` | `Large` | qualité avant latence et ressources |

```rust
let mut request = AiRequest::text(
    AiTask::GenerateText,
    "Comparez les alternatives et justifiez la conclusion.",
    AiLimits::default(),
)?;
request.options.quality = AiQualityTarget::Deep;
request.options.latency = AiLatencyClass::Balanced;
request.options.execution = AiExecutionMode::Auto;
request.options.allow_escalation = true;
request.options.deadline = Some(Duration::from_secs(45));

let answer = ai.resolve(request).await?;
```

Un model ID forcé contourne le filtre automatique de qualité, mais doit encore
respecter format, modalités, device, privacy, ressources et deadline. Aucun
downgrade silencieux de modèle ou de quantification n'est permis.

`Deep` n'autorise ni boucles autonomes illimitées ni exposition de chain of
thought. Un futur planner multiétape pourra exécuter draft, vérification et
synthèse avec des bornes de steps, tokens, coût et deadline. L'application
reste propriétaire des prompts, tools, schemas et policies métier.

## Images et PDF

La beta valide des modalités de premier niveau :

```rust
let input = AiInput::new(
    vec![
        AiContent::Text("Listez les risques visibles dans cette image".into()),
        AiContent::Binary {
            media_type: "image/png".into(),
            bytes: image_bytes,
        },
    ],
    limits,
)?;
let request = AiRequest {
    task: AiTask::AnalyzeImage,
    input,
    options: AiOptions::default(),
};
```

Pour PDF :

```rust
let input = AiInput::new(
    vec![AiContent::Binary {
        media_type: "application/pdf".into(),
        bytes: pdf_bytes,
    }],
    limits,
)?;
let request = AiRequest {
    task: AiTask::AnalyzeDocument,
    input,
    options: AiOptions {
        quality: AiQualityTarget::Deep,
        ..AiOptions::default()
    },
};
```

Un PDF est un conteneur, pas une modalité native de chaque VLM. Un adapter peut
utiliser le support documentaire natif, extraire du texte borné avec références
de pages, rasteriser les seules pages admises ou appliquer un OCR borné. Un
processor document limitera pages, pixels, octets décompressés, durée et output.
Le core n'embarque ni parser PDF, ni OCR, ni décodeur d'image, et ne télécharge
jamais implicitement les ressources externes d'un document.

## Système décisionnel

`AiTask::Decide` ne rend pas une completion automatiquement autoritaire :

```text
règle déterministe
  -> petit classificateur si la règle ne suffit pas
  -> LLM/VLM seulement pour l'ambiguïté permise
  -> validation du schema et de confidence
  -> la policy applicative accepte, refuse ou demande une revue
```

Règles, outcomes et thresholds appartiennent à l'application. Le Runtime offre
bornes, routage, audit expurgé et diagnostic de route. Toute action privilégiée
doit exiger un output structuré, des preuves référencées et un fallback sûr,
jamais du texte généré brut.

## Profils de serveurs pris en charge

La feature `backend-openai-compatible` possède un profil explicite pour chaque
famille ci-dessous. Le moteur reste un processus déployé séparément ; la feature
ne l'installe, ne le télécharge, ne le démarre et ne le sandboxe pas. Le
deployment déclare modèles, devices, vision/tools/seed/stop, endpoint et bornes.

| Matériel/workload | Adapter conseillé | Pourquoi | Compromis |
|---|---|---|---|
| CPU, GGUF, matériel varié | [llama.cpp](https://github.com/ggml-org/llama.cpp) | couverture, quantification, CPU/GPU hybride | pas toujours le plus rapide par device |
| Apple Silicon | [MLX-LM](https://github.com/ml-explore/mlx-lm) et driver VLM MLX | mémoire unifiée, kernels natifs | spécifique Apple |
| NVIDIA grand public, faible concurrence | [ExLlamaV3](https://github.com/turboderp-org/exllamav3) via TabbyAPI | EXL3 et GPU consumer | format/écosystème spécialisé |
| NVIDIA/AMD, forte concurrence | [SGLang](https://www.sglang.io/) ou [vLLM](https://docs.vllm.ai/en/stable/) | batching continu, KV cache, multimodal | stack opérationnelle plus grande |
| NVIDIA, performance maximale | [TensorRT-LLM](https://docs.nvidia.com/tensorrt-llm/) | kernels, quantification, serving NVIDIA | couplage matériel fort |
| Intel CPU/GPU/NPU | [OpenVINO GenAI](https://docs.openvino.ai/2026/openvino-workflow-generative/inference-with-genai.html) | pipelines LLM/VLM optimisés | conversion et formats dédiés |
| petit classificateur Rust | backend Candle actuel | in-process, data-only, auditable | pas un LLM génératif |

La sélection mesure le tuple complet :

```text
engine version + model revision + quantization + context + batch + device
```

Elle conserve au minimum cold start, TTFT, prompt tokens/s, decode tokens/s,
requests/s, RAM, VRAM, queue depth et taux d'erreur. Une configuration ne gagne
que pour la classe de workload réellement mesurée.

## Adapter serveur commun livré

`OpenAiCompatibleBackend` traduit une fois le contrat central pour les serveurs
listés. Il fournit :

- endpoint loopback par défaut ;
- binding exact `ModelId` vers nom de modèle serveur ;
- messages avec rôles, sampling borné, tools/tool calls et data URLs image ;
- erreur explicite pour toute capability non déclarée ;
- tailles request/response, timeout et cancellation bornés ;
- admission équitable et bornée autour des routes modèle ;
- erreurs stables sans body provider ni sortie privée du processus ;
- statut HTTP exact et `Retry-After` borné en secondes, sans body provider ;
- arguments bruts de tool call récupérables avec metadata finish et usage ;
- profils provider validés pour omettre sampling, choisir le champ de limite de
  tokens et ajouter du JSON borné sans remplacer les champs réservés ;
- response format JSON Schema opt-in avec fallback reject ou JSON-text explicite ;
- trait de transport asynchrone ne recevant qu'une référence de secret AppCore ;
- SSE opt-in via `AiRuntime::resolve_stream`, backpressure synchrone et
  annulation coopérative. Après l'émission d'un événement, un échec transitoire
  est retourné sans mélanger la sortie d'une route fallback.

Le transport HTTP par défaut refuse toute référence de credential et convient
uniquement aux endpoints loopback/privés sans authentification. Un deployment
distant fournit un transport AppCore security et utilise le constructeur
explicite `OpenAiCompatibleConfig::remote`. Le processus moteur reste chargé ;
lancement, health probe et sandbox OS appartiennent au deployment.

```bash
APPCORE_AI_BASE_URL=http://127.0.0.1:8080 \
APPCORE_AI_MODEL=mon-modele \
APPCORE_AI_MODEL_SHA256=<digest-hexadecimal-64-caracteres> \
cargo run -p appcore-ai --example openai_compatible \
  --features backend-openai-compatible
```

## Résidence propre à AppCore

La résidence du modèle complet utilise toujours :

```text
ArtifactIdentity -> Vram(device) | Memory | LocalStorage | Peer(peer)
```

`ArtifactBundleManifest` et `SegmentedModelReader` implémentent maintenant la
frontière de ranges indépendante du moteur :

```text
ModelBundle
  dense/tokenizer/config  -> résident de préférence
  segment 000..N          -> digest + taille + offset + classe
  access observations     -> hot/warm/cold borné
  placement               -> VRAM -> RAM -> mmap/NVMe -> peer vérifié
```

Le lecteur valide ranges ordonnés sans chevauchement, bornes par segment/request
et SHA-256 de chaque segment, puis `LocalArtifactCache::load_range` évite
d'allouer l'artifact complet. Le core planifie octets et tiers, l'adapter garde
tensors et kernels. Prefetch, cache, eviction, rollback et I/O
pressure restent bornés et observables. Aucun peer ne force une résidence
locale ; aucun `LD_PRELOAD`, hook filesystem ou format caché de tiers n'est
utilisé. Sans backend consommateur, le projet ne revendique pas encore le
streaming d'experts et le governor refuse un modèle qui ne tient pas.

## Premières familles de modèles

| Modèle | Modalités/usage | Profil initial |
|---|---|---|
| [Qwen3 4B](https://huggingface.co/Qwen/Qwen3-4B) | texte local multilingue | `Fast`/`Balanced`, GGUF Q4/Q5 |
| [Qwen3 8B](https://huggingface.co/Qwen/Qwen3-8B) | réponses générales plus fortes | `Balanced`/`Deep` |
| [Gemma 3 12B IT](https://huggingface.co/google/gemma-3-12b-it) | texte et image | VLM `Deep`; accepter les termes Gemma |
| [Phi-4 multimodal](https://huggingface.co/microsoft/Phi-4-multimodal-instruct) | texte, image et audio | SafeTensors/OpenVINO après conformance |
| [Mistral Small 3.2 24B](https://huggingface.co/mistralai/Mistral-Small-3.2-24B-Instruct-2506) | texte, vision, tools, documents | `Deep`/`Maximum`, matériel plus grand |

N'activez jamais `trust_remote_code`. Un modèle custom-code nécessite un
adapter et une policy explicitement revus. Voir
[modèles et training](models.fr.md) pour le support effectivement livré.

## Livré et gates externes

Livré dans la beta : modalités/qualité, chat avec rôles, sampling/tools bornés,
adapter commun, sept profils moteur, test loopback réel, admission équitable,
manifests/ranges segmentés et composition Supervisor/capabilities opt-in dans
`appcore-bin`.

Le streaming natif de tokens exige un transport de deployment qui implémente
cette frontière ; le transport HTTP par défaut ne fournit qu'une réponse
complète hors du thread executor. Restent non revendiqués : accounting KV cache
moteur, installation automatique/sandbox processus, PDF/OCR, expert streaming, adapter Swarm Peer RPC
de production ou manifest V2 déclaratif. Ce sont des gates explicites, pas des
promesses documentaires. Le core explique pourquoi une route est admise ou
refusée sans promettre chaque modèle sur chaque machine.
