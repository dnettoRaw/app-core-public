# appcore-ai

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md) |
[Exemple de base](wiki/examples/basic.fr.md) |
[Exemple Candle](wiki/examples/intermediate.fr.md) |
[Recettes](wiki/recipes.fr.md) |
[Modèles](wiki/models.fr.md) |
[LLM génératifs](wiki/generative-llm.fr.md) |
[Ressources matérielles](wiki/resources.fr.md) |
[Performance](wiki/benchmarks.fr.md)

Orchestration IA bornée et indépendante du backend pour AppCore Runtime, avec
SemVer indépendant. La release actuelle est `0.1.0-beta.3` ; elle ne modifie
aucun manifest ni contrat wire AppCore V1 gelé.

La compilation par défaut fournit requêtes/réponses validées, modalités
explicites, profils de qualité, chemin lightweight déterministe, gouvernance
des ressources, scheduler par coût, files équitables et batching bornés, load
single-flight par modèle/backend, registres modèles/artefacts, résidence par
tiers, frontières de provenance, télémétrie expurgée et API asynchrone
`AiRuntime::resolve`. Elle ne dépend d'aucun framework ML.

La release beta fournit aussi batching adaptatif au backend, batch Candle
vectorisé, coordination LRU bornée des loads et `ModelLoadSnapshot` public.
Les artefacts locaux utilisent ouverture no-follow, revalidation du handle et
activation atomique sans remplacement. Registres, routes apprises, résidence,
loads et claims Swarm ont des limites fixes.

Les features optionnelles sont explicites :

- `accelerator-nvidia` : détection NVIDIA VRAM/utilisation en lecture seule via
  NVML chargée dynamiquement sous Linux/Windows ; absente du graphe par défaut ;
- `backend-candle` : inférence CPU réelle pour `NativeLinearV1` borné ;
- `backend-openai-compatible` : transport chat-completions réel et borné pour
  llama.cpp, MLX-LM, TabbyAPI, vLLM, SGLang, TensorRT-LLM, OpenVINO ou un
  serveur compatible explicitement testé ;
- `training-candle` : SGD local reproductible, checkpoints atomiques et reprise ;
- `swarm` : bridge authentifié expérimental, vues peers expirantes,
  contributions compute/storage séparées et failover.

Le contrat génératif inclut chat avec rôles, sampling borné, outils/tool calls
typés et images. L'adaptateur HTTP exécute texte/chat et, si le serveur/modèle
déclare cette capacité, l'analyse d'image. PDF est une modalité de premier
ordre mais exige encore un backend document choisi par l'application ; le core
n'embarque pas de parseur PDF/OCR universel dangereux. `SegmentedModelReader`
lit des ranges avec digest par segment sans prétendre que tout moteur sait faire
de l'expert streaming.

Cette release renforce la frontière OpenAI-compatible avec statut HTTP typé
et `Retry-After` borné, arguments bruts de tool call récupérables, futures de
transport réellement asynchrones, profils de compatibilité validés, sortie JSON
Schema opt-in et streaming annulable avec backpressure synchrone. Le streaming
n'existe que si capability et transport du deployment le déclarent ; le client
HTTP bloquant par défaut est déplacé hors du thread executor et ne prétend pas
livrer le réseau incrémentalement.

Swarm ne crée jamais un second control plane ni une seconde authentification.
L'adaptateur hôte doit utiliser la sécurité, les capabilities et Peer RPC
AppCore. Le calcul distant exige des grants tenant explicites et les octets
d'artefact peer sont vérifiés avant activation.

```bash
cargo test -p appcore-ai
cargo test -p appcore-ai --all-targets --all-features
./crates/appcore-ai/scripts/check-feature-matrix.sh
cargo test -p appcore-ai --test stress_soak --all-features
APPCORE_AI_BENCH_FORMAT=jsonl cargo bench -p appcore-ai --bench perf_lab --all-features
```

`Unrestricted` retire seulement la marge volontaire AppCore. Il ne désactive
aucune protection OS, pilote, firmware, thermique ou électrique et ne garantit
pas l'absence de throttling.

Exemples exécutables :

```bash
cargo run -p appcore-ai --example lightweight_runtime
cargo run -p appcore-ai --example hardware_report
cargo run -p appcore-ai --example candle_runtime --features backend-candle
cargo run -p appcore-ai --example openai_compatible --features backend-openai-compatible
cargo run -p appcore-ai --example candle_training --features training-candle
```

La feature délibérément expérimentale `appcore-bin/ai-alpha` livre un flux
explicite via Supervisor et `CapabilityRegistry` sans modifier V1. La sélection
déclarative reste un travail post-1.0 hors de la portée beta. Voir le
[rapport release](wiki/release-readiness.fr.md) et le
[threat model](wiki/threat-model.fr.md).
