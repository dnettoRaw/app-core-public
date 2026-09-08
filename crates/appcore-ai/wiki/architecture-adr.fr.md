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

## Éléments comparatifs

La recherche utilise documentation et articles disponibles à la date de l'ADR.
Les performances des projets ne sont pas des résultats AppCore ; chaque
optimisation exige son propre benchmark reproductible.

| Projet | Technique | Bénéfice | Coût ou contrainte | Décision AppCore | Phase |
|---|---|---|---|---|---|
| Lumabri | Dons storage/compute indépendants, lecture à la demande, miroir local partiel et failover des répliques | Contribution de peers CPU-only/storage-only et conservation locale des octets fréquents | Réseau/sécurité expérimentaux ; premier accès dépendant du réseau | Séparer `ArtifactStore` et `ComputeTarget`, annonces expirables et cache local vérifié ; aucun hook filesystem | beta expérimentale |
| llama.cpp | GGUF, quantification étendue, offload CPU/GPU hybride, mmap et batching continu serveur | Portabilité locale et fonctionnement avec VRAM insuffisante | Build C/C++, serveur évoluant rapidement et frontière de crash natif | Profil OpenAI-compatible livré ; cycle de vie du processus au déploiement | adapter beta |
| vLLM | PagedAttention, batching continu, cache préfixe/KV et serving distribué | Haut débit et moindre fragmentation KV en génération concurrente | Stack Python/GPU lourde ; techniques spécifiques aux workloads | Batching par clé de compatibilité et comptabilité de cache bornée, pas son API/runtime | optimisation ultérieure |
| SGLang | Réutilisation de préfixes RadixAttention, batching continu, prefill par chunks et séparation prefill/decode | Préfixes répétés et workloads mixtes efficaces | Stack serving lourde et scheduling complexe des accélérateurs | Extensions prefix-cache et étapes séparées privées jusqu'à une demande mesurée | optimisation ultérieure |
| Burn | Modèles/training Rust, backends tensoriels interchangeables et import ONNX | Entraînement optionnel cohérent et portabilité | Coût de compilation/dépendances ; sémantique du modèle définie par l'application | Évalué, non retenu ; éviter un second framework sans besoin mesuré | recherche uniquement |
| Candle | Runtime tensoriel Rust ; CPU, CUDA, Metal et WASM ; chargement safetensors/ggml | Intégration Rust et inférence locale portable | Intégration modèle/tokenizer spécifique au modèle | Premier backend CPU et trainer opt-in ; types Candle hors contrats centraux | adapter beta |
| ONNX Runtime | Découverte des capabilities Execution Provider, partitionnement du graphe et arènes mémoire | Exécution mature sur CPU, GPU et plusieurs NPU | Distribution native et compatibilité providers ; API tensorielle distincte d'une API texte | Sélection du device par capability ; candidat backend tensoriel borné | backend futur |
| TensorRT-LLM | Batching inflight, cache KV paginé, quantification, multi-GPU/multi-node | Haut débit NVIDIA et optimisations serving matures | Spécialisation NVIDIA/Linux et empreinte opérationnelle importante | Profil serveur compatible livré ; optimisations moteur hors du core | adapter beta, moteur externe |

Le pipeline public est borné et observable :

```text
valider la requête
  -> classifier et valider les modalités d'entrée
  -> essayer les resolvers légers déterministes
  -> trouver les modèles compatibles
  -> appliquer le minimum de qualité Fast/Balanced/Deep/Maximum
  -> calculer les budgets local et de contribution
  -> planifier le placement des artefacts
  -> planifier le placement du calcul
  -> admettre dans une file/batch borné
  -> exécuter par backend
  -> escalade optionnelle avec plafond de tentatives
  -> retourner un trace expurgé sur demande
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

`Auto` peut comparer ou combiner ressources locales et distantes, mais
n'autorise pas un transfert silencieux : confidentialité et distribution
restent gouvernées par les policies. Les octets des peers sont bornés et
vérifiés avant activation. L'entrée publique `AiRuntime::resolve` est
asynchrone et observable ; l'escalade a des tentatives bornées et le trace
expurgé est opt-in.

- contrats : requêtes/réponses/options validées, IDs, policies, limites,
  annulation et diagnostic sûr ;
- résolution légère : transformations, règles, matching et extraction bornés ;
- routeur : ordre des candidats, escalade bornée et application des policies ;
- governor : snapshots des probes, hystérésis et budgets local/contribution séparés ;
- scheduler : admission et score déterministe local/distant ;
- registry : métadonnées, cycle de vie, capabilities et identité/localisation des artefacts ;
- backend SPI : load/unload/inference/health et entraînement spécialisé ;
- batching : files compatibles bornées, délais, annulation et échecs partiels ;
- résidence : promotion, prefetch et éviction bornés par tier de stockage supporté ;
- pont distribué : vues authentifiées et expirables des peers, invocation de
  calcul et frontières de transfert des artefacts ;
- composition root : providers, capabilities, Supervisor et policy deployment.

## Décisions de l'alpha

- Le core et les tests déterministes fonctionnent sans GPU, réseau ni download.
- Une capacité inconnue n'est jamais considérée comme illimitée.
- `Unrestricted` retire seulement la marge volontaire AppCore, pas les
  protections OS, driver, firmware, thermiques ou électriques.
- Files, retries, peers, métadonnées, entrées, sorties, artefacts et transferts
  possèdent des bornes explicites.
- Un peer distant n'est pas un backend : backend décrit comment exécuter,
  target où et store où résident les octets.
- Candle `0.11` est le seul framework ML retenu, uniquement via
  `backend-candle` et `training-candle`.
- Le premier format est le classificateur data-only et borné `NativeLinearV1` ;
  aucun type Candle ne traverse l'API centrale.
- l'intégration explicite de déploiement livre une composition Supervisor/CapabilityRegistry
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
- lifecycle/capability opt-in réel dans le déploiement.

Le streaming natif exige un transport de deployment explicitement capable.
Restent hors claim : PDF/OCR, lancement ou sandbox automatique, accounting KV cache moteur, expert streaming sans backend
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

## Conséquences

La décision ajoute des frontières d'orchestration avant les moteurs coûteux et
exige davantage de configuration explicite. Certains modes retournent une
erreur typée d'indisponibilité. En échange, le core par défaut reste portable
et testable, les changements de backend restent derrière le SPI et les futures
intégrations peuvent utiliser un nouveau contrat versionné sans affaiblir V1.
Le coût optionnel de Candle reste explicite, sans alourdir le build minimal.

## Amendement beta.2 du 2026-08-25

Le SPI OpenAI-compatible retourne désormais des futures boxed afin qu'un
deployment fournisse un HTTP asynchrone natif sans imposer un executor au core.
Le client borné par défaut isole le transport standalone bloquant derrière un
maximum de threads courts. Il ne bloque pas le caller executor et rejette
l'excès au lieu de créer une file non bornée.

Le streaming utilise un `AiStreamSink` synchrone : le retour d'un événement
autorise la lecture du chunk suivant. La backpressure est explicite sans channel
spécifique au runtime. L'annulation est vérifiée entre chunks, un output partiel
n'est jamais une réponse complète et le contenu brut reste hors diagnostics.
Les extensions provider sont du JSON borné avec champs centraux réservés ; le
fallback JSON Schema est toujours choisi par l'appelant. Aucun manifest ni
contrat wire V1 ne change.
