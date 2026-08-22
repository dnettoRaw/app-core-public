# Guide appcore-ai

[English](guide.en.md) | [Português](guide.pt.md) |
[Exemple basic](examples/basic.fr.md) |
[Exemple intermédiaire](examples/intermediate.fr.md) |
[Recettes concrètes](recipes.fr.md) |
[Modèles et training](models.fr.md) |
[LLM génératifs](generative-llm.fr.md) |
[Ressources matérielles](resources.fr.md) |
[ADR](architecture-adr.fr.md) | [Threat model](threat-model.fr.md) |
[État de release](release-readiness.fr.md)

`appcore-ai` possède l'orchestration IA générique et bornée. Prompts, schémas,
identifiants secrets et workflows métier appartiennent aux applications. La
crate a son propre SemVer ; cette release est `0.1.0-beta.1`.

## Parcours d'apprentissage

1. Exécutez le [runtime lightweight](examples/basic.fr.md) sans feature optionnelle.
2. Exécutez [Candle via AiRuntime](examples/intermediate.fr.md).
3. Configurez ou entraînez un classifieur dans
   [modèles et training](models.fr.md).
4. Lisez le [runtime adaptatif](generative-llm.fr.md) pour texte, vision, PDF,
   sélection multi-engine, résidence propre et modèles conseillés.
5. Utilisez les [recettes concrètes](recipes.fr.md) pour ressources, cache,
   annulation, Swarm, training, observabilité et backpressure.
6. Avant la production, lisez le [threat model](threat-model.fr.md) et
   l'[état de release](release-readiness.fr.md).

Les exemples `lightweight_runtime`, `candle_runtime`, `openai_compatible` et
`candle_training` sont
des sources compilés sous `examples/` ; les principaux snippets de la wiki en
sont dérivés et incluent commandes et sorties attendues.

## Architecture et résolution

```text
AiRuntime::resolve
  -> validation modalité/contenu/confidentialité/autorisation/limites
  -> resolver lightweight déterministe
  -> applique le minimum Fast/Balanced/Deep/Maximum
  -> registre modèle et identité artefact
  -> admission ResourceGovernor
  -> scheduler coût (CPU/GPU/NPU/peer autorisé)
  -> admission d'exécution équitable et bornée
  -> `infer` ou `infer_batch` compatible coordonné explicitement
  -> résidence (VRAM -> RAM -> local -> peer)
  -> backend ou SwarmBridge
  -> escalation bornée
  -> diagnostic et télémétrie expurgés
```

Le scheduler combine charge, marge mémoire, profondeur de file, résidence,
coût load/transfert, EMA latence/throughput, priorité, deadline et mode. Poids
entiers et horloges injectables rendent les tests déterministes. Compute et
storage restent deux décisions distinctes.

Le chemin lightweight normalise le texte et applique des règles explicites et
bornées. Il déclare raison/certitude, répond directement ou conserve un
fallback sûr avant escalation.

## Ressources et modes

`ResourceGovernor` utilise `HardwareProbe`, cache et hystérésis. RAM/VRAM
inconnue n'est jamais infinie. Budgets locaux et dons sont distincts ;
`AiContributionPolicy` désactive compute et storage séparément.

`SystemHardwareProbe::default()` lit CPU/RAM réels sous macOS, Linux et
Windows. Il découvre le GPU Apple à mémoire unifiée, les devices DRM Linux et,
avec `accelerator-nvidia`, VRAM/utilisation NVIDIA via NVML. Le fit par device
exact interdit d'additionner des GPU indépendants. La
[page ressources](resources.fr.md) détaille matrice, rapport exécutable, coût
de dépendance et sémantique opérationnelle.

| Mode | Politique volontaire AppCore |
|---|---|
| `Eco` | marge maximale, petits batches |
| `Balanced` | interactivité et training conservateur |
| `Performance` | throughput avec marge de sécurité |
| `Unrestricted` | retire la marge volontaire dans les limites backend/OS |
| `Custom` | plafonds explicites validés |

`Unrestricted` ne désactive aucune protection OS, pilote, firmware, thermique
ou électrique et ne garantit pas l'absence de throttling. Files, batches,
essais, peers, artefacts, transferts, contenus, workers et jobs sont bornés.

## Modèles, artefacts et résidence

`ModelRegistry` sépare metadata, lifecycle et emplacements. `ArtifactIdentity`
utilise SHA-256, taille exacte et publisher optionnel. Le cache local écrit un
temporaire exclusif, synchronise et active atomiquement ; aucun nom peer n'est
fiable.

```text
ArtifactIdentity -> Vram(device) | Memory | LocalStorage | Peer(peer)
```

`ResidencyPlanner` offre réutilisation LRU simple, éviction en deux phases,
prefetch borné, fallbacks et rollback. Une charge concurrente voit `InFlight`.

## Backend et training optionnels

Le default ne contient aucun framework ML. `backend-candle` active une vraie
inférence CPU pour `NativeLinearV1` data-only : load vérifié, unload,
concurrence, annulation et métriques, sans téléchargement automatique.

```bash
cargo run -p appcore-ai --example candle_cpu --features backend-candle
```

`training-candle` ajoute SGD local reproductible. Dataset, dimensions, labels,
epochs, steps, batch, ressources et checkpoints sont bornés ; reprise et
activation atomique sont supportées. Le training distribué ne l'est pas.

`backend-openai-compatible` est le chemin génératif réel vers un serveur
llama.cpp, MLX-LM, TabbyAPI, vLLM, SGLang, TensorRT-LLM, OpenVINO ou compatible
testé, exécuté séparément. Il prend en charge chat avec rôles, sampling borné,
tools/tool calls et image déclarée. Le transport par défaut est loopback-first
sans authentification ; un credential distant exige un adapter AppCore security.

## Local, Swarm et Auto

`swarm` est expérimental et exige un `SwarmBridge` authentifié composé par
l'hôte AppCore.

```text
nœud storage-only -> ArtifactStore(peer) ----+
nœud compute-only -> ComputeTarget(peer) ----+-> Auto planner -> exécution
nœud combiné      -> les deux ---------------+
nœud local        -> CPU/GPU/NPU + cache ----+
```

- `Local` ne consulte jamais de peer.
- `Swarm` exige une route distante autorisée et échoue fermé.
- `Auto` compare les coûts permis ; la confidentialité locale gagne toujours.

Les annonces expirent et ne publient que le budget après contribution policy.
Compute distant exige `ai.remote.compute`, storage distant
`ai.remote.storage`. Le transfert d'artefact est distinct du Peer RPC générique.
Le failover est borné. La correction d'un résultat distant n'est généralement
pas vérifiable cryptographiquement ; cette limite est explicite.

## Sécurité et observabilité

`ModelSecurityPolicy` refuse par défaut les formats provider/custom-op et borne
artefact/RAM/VRAM. `ProvenanceArtifactStore` délègue les signatures à la
sécurité AppCore. `Debug` expurge prompts, contenus, outputs, embeddings,
labels et valeurs metadata. Les credentials restent des références.

`AiTelemetry` expose p50/p95/p99 à buckets fixes, outcomes, admissions, loads,
fallback/escalation et placements sans labels à forte cardinalité.
`AiObservationSink` est le point d'adaptation `appcore-ops`.

Des snapshots par composant complètent cette vue bornée :
`FairQueueMetrics` et `BatcherMetrics` exposent profondeur, saturation et items ;
`ResidencyMetrics` expose réutilisations, chargements en cours, rollbacks,
évictions et octets ; `PeerArtifactMetrics` expose les octets distants vérifiés ;
`PeerDirectoryMetrics` expose disponibilité, contribution et churn agrégés.
Placement et progression training restent également sans labels d'ID arbitraire.

`AiRuntime::model_loads()` expose gauges ready/loading et compteurs de hit,
waiter, loader, eviction et invalidation. Ils détectent loads froids répétés ou
route bloquée en loading sans exposer d'ID modèle/backend.

## Niveaux de l'API publique

Les exports plats sont classés par usage, pas par promesse de stabilité :

| Niveau | Types typiques | Appelant |
|---|---|---|
| Essentiel | `AiRuntime`, requête/réponse/sortie, options, limites, annulation et erreurs | applications résolvant du travail IA borné |
| Politique avancée | governor/admission, registres, scheduler, files, batching, résidence, artefacts, bundles, télémétrie et sécurité | composition root réglant placement et ressources |
| SPI backend | `InferenceBackend`, descriptors/futures, `ArtifactStore`, peer transport, observations, planners, training et transport OpenAI optionnels | adaptateurs backend/provider/hôte |
| Interne | création des routes, permits de load, execution queue, scoring et codecs HTTP | implémentation du crate ; non exporté |

Le graphe default n'inclut aucun moteur ML ou HTTP. `sha2` fournit l'identité
artefact ; `libc` ou `windows-sys`, propres à la cible, fournissent les flags
no-follow sûrs et les compteurs de ressources natifs. `nvml-wrapper` est isolé
derrière `accelerator-nvidia` ; Candle et OpenAI-compatible restent sous
features explicites.
`#![deny(unsafe_code)]` s'applique au crate ; la FFI native documentée et
strictement limitée n'est permise que dans les modules de ressources macOS et
Windows. La découverte Linux et le wrapper NVIDIA optionnel utilisent des API
sûres.

## Preuves de performance et charge

`perf_lab` couvre resolve lightweight/miss/froid/chaud, scaling 1/32/128 des
registres et scheduler, batch 1/2/4/8/16, artefact full/range, batch Candle
1/8/32, training et Swarm 1/10/100/1 000. Produisez JSONL et stress ainsi :

```bash
APPCORE_AI_BENCH_FORMAT=jsonl \
  cargo bench -p appcore-ai --bench perf_lab --all-features
APPCORE_AI_SOAK_ITERATIONS=100000 \
  cargo test -p appcore-ai --test stress_soak --all-features -- --nocapture
```

Voir le [rapport d'optimisation](benchmarks.fr.md) pour avant/après, mémoire,
coût volontaire du durcissement artefact et limites d'interprétation.

## Modèles d'utilisation

1. `resolve()` lightweight avec `AiRequest::text(TransformText, ..)`.
2. Modèle local forcé avec `execution = Local` et `options.model`.
3. Ressources custom via `AiResourceMode::Custom`.
4. `Unrestricted` seulement après acceptation du risque pression/throttling.
5. Classificateur optionnel via `examples/candle_cpu.rs` et `backend-candle`.
6. LLM optionnel via `examples/openai_compatible.rs`.
7. Training via `TrainingJob`, `TrainingDataset`, admission et `CandleTrainer`.

## Limites et gates

Il n'existe ni champ manifest V1 ni option CLI. La feature opt-in
`appcore-bin/ai-alpha` ajoute `ManifestApplicationHost::with_ai`, la façade
`ApplicationAi`, le handler `appcore.ai.resolve` et le lifecycle graceful du
Supervisor sans modifier V1. La sélection déclarative exige encore un contrat
post-1.0 accepté. Transport, authentification, replay et isolation appartiennent
au host/deployment ; la crate ne prétend ni sandbox ni zero trust.

```bash
cargo test -p appcore-ai --all-targets --all-features
./crates/appcore-ai/scripts/check-feature-matrix.sh
cargo bench -p appcore-ai --bench perf_lab --all-features
```
