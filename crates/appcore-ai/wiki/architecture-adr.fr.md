# ADR 0001 : architecture d'orchestration AppCore AI

- Statut : accepté pour l'implémentation `0.1.0-beta.1`
- Date : 2026-08-21
- Périmètre : `appcore-ai`, sans modification des manifests ou du protocole V1

[Profil LLM génératif](generative-llm.fr.md) |
[Modèles et training](models.fr.md)

## Contexte et décision

`appcore-ai` sera une crate de la couche Runtime avec un SemVer indépendant.
La compilation par défaut restera légère et utile sans LLM. Les frameworks
d'accélération et d'entraînement seront optionnels et leurs types ne seront
jamais exposés par les contrats centraux.

La recherche compare les sources primaires de
[Lumabri](https://github.com/JustVugg/lumabri),
[llama.cpp](https://github.com/ggml-org/llama.cpp),
[vLLM](https://docs.vllm.ai/),
[SGLang](https://github.com/sgl-project/sglang),
[Burn](https://burn.dev/books/burn/),
[Candle](https://huggingface.github.io/candle/),
[ONNX Runtime](https://onnxruntime.ai/docs/reference/high-level-design.html) et
[TensorRT-LLM](https://nvidia.github.io/TensorRT-LLM/).

| Source | Bénéfice retenu | Limite conservée |
|---|---|---|
| Lumabri | dons storage/compute séparés, miroir local et failover | aucun hook filesystem ni protocole parallèle |
| llama.cpp | portabilité, quantification et CPU/GPU hybride | backend optionnel et isolé seulement |
| vLLM/SGLang | batching compatible et comptabilité KV/prefix cache | seulement après benchmarks AppCore |
| Burn | workflow Rust d'entraînement/inférence | évalué mais non retenu afin d'éviter un second framework |
| Candle | intégration Rust pour entraînement/inférence | retenu uniquement dans les features beta optionnelles |
| ONNX Runtime | sélection par capability des execution providers | l'API tensor ne devient pas l'API texte centrale |
| TensorRT-LLM | batching et KV cache à haut débit | intégration NVIDIA future uniquement |

Le pipeline public est borné et observable :

```text
validation modalité -> voie légère -> minimum qualité -> modèles -> budget -> placement artefact
           -> placement calcul -> admission -> exécution -> escalade bornée
```

Dès le premier alpha :

```rust
pub enum AiExecutionMode {
    Local,
    Swarm,
    Auto,
}
```

Le calcul et le stockage sont deux décisions indépendantes.
`InferenceBackend` décrit comment exécuter, `ComputeTarget` où exécuter et
`ArtifactStore` où résident les octets. L'identité d'un artefact dérive de son
contenu et ne change pas avec sa localisation.

## Responsabilités

- contrats : requêtes, réponses, IDs, policies, limites et diagnostic sûr ;
- résolution légère : transformations, règles, matching et extraction bornés ;
- routeur : candidats et escalade avec une limite fixe ;
- governor : budgets local et de contribution séparés ;
- scheduler : admission et score déterministe local/distant ;
- registry : métadonnées, cycle de vie et localisations des artefacts ;
- backend SPI : load/unload/inference/health et entraînement spécialisé ;
- batching/résidence : files, promotion, prefetch et éviction bornés ;
- pont distribué : peers authentifiés et annonces expirables ;
- composition root : providers, capabilities, Supervisor et policy deployment.

## Décisions de l'alpha

- Candle `0.11` est le seul framework ML retenu, uniquement via
  `backend-candle` et `training-candle`.
- Le premier format est le classificateur data-only et borné `NativeLinearV1` ;
  aucun type Candle ne traverse l'API centrale.
- `appcore-bin/ai-alpha` livre une composition Supervisor/CapabilityRegistry
  explicite sans modifier V1 ; la sélection déclarative attend un contrat
  post-1.0 versionné.

## Décision de détection des ressources

La découverte matérielle est une petite frontière de plateforme derrière
`HardwareProbe`, pas un nouveau framework/provider. La topologie statique est
cachée séparément des compteurs dynamiques et `HardwareSampler` fonctionne à
la demande, en single-flight et avec bornes. Les inconnues restent inconnues ;
les échecs deviennent des catégories stables et expurgées.

CPU/RAM utilisent des interfaces OS natives bornées. Le GPU Apple intégré est
modélisé en mémoire unifiée. DRM sysfs fournit AMD et fallback NVIDIA en
best-effort. NVML reste derrière `accelerator-nvidia`, car les API OS portables
n'exposent pas exactement mémoire framebuffer et utilisation NVIDIA. Seules
des queries en lecture sont utilisées ; aucun contrôle fréquence/puissance/fan.

L'admission vise un device exact. La VRAM dédiée n'est jamais additionnée entre
GPU ; la mémoire unifiée est débitée une seule fois du pool RAM. Les modes
calculent leur marge volontaire depuis la disponibilité et l'hystérésis réduit
les oscillations. Batching, entraînement, résidence et don Swarm partagent la
même vue bornée.

Cela requiert une FFI native petite et documentée sous macOS et Windows. Le
crate utilise `#![deny(unsafe_code)]`, avec `allow` seulement dans ces modules.
Le reste demeure en Rust sûr. Voir [ressources matérielles](resources.fr.md).

## Implémentation générative beta et limites restantes

La beta livre :

- adapter OpenAI-compatible borné et sept profils serveur explicites ;
- chat avec rôles, sampling, tools/tool calls, usage et image opt-in ;
- moteur externe persistant, loopback par défaut, aucun download à l'inférence ;
- manifests de segments AppCore et ranges locaux vérifiés ;
- load single-flight par modèle/backend lors du fallback et en concurrence ;
- lifecycle/capability opt-in réel dans `appcore-bin`.

Restent hors claim : streaming de tokens, PDF/OCR, lancement ou sandbox
automatique, accounting KV cache moteur, expert streaming sans backend
consommateur et manifests V2 déclaratifs.

Cette frontière garde crashes natifs, tokenizers, KV cache et kernels hors du
core backend-neutral. Le [profil génératif](generative-llm.fr.md) contient
modèles, budgets, commandes et gates.

## Hors de `0.1.0`

- un framework deep learning ou tensoriel développé en interne ;
- téléchargements silencieux, files/transferts non bornés ou custom ops sûrs
  par simple affirmation ;
- entraînement distribué, consensus, traversée NAT ou second control plane ;
- extension silencieuse des contrats V1 ;
- prétendre que `Unrestricted` désactive les protections matérielles ;
- promotion RC/stable sans les preuves exigées.

Le Swarm devient opérationnel uniquement avec un pont authentifié. Les peers
simulés prouvent le planner, pas un réseau de production. Le runtime peut
vérifier les artefacts et authentifier les peers, mais ne promet pas une preuve
cryptographique générale de la correction d'un résultat distant. Activer
Candle agrandit sensiblement l'arbre optionnel des dépendances ; la compilation
par défaut reste sans framework ML.
