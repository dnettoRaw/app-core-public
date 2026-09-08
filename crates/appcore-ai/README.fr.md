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
aucun manifest ni contrat wire AppCore V1 stable.

La compilation par défaut fournit requêtes/réponses validées, modalités
explicites, profils de qualité, chemin lightweight déterministe, gouvernance
des ressources, scheduler par coût, files équitables et batching bornés, load
single-flight par modèle/backend, registres modèles/artefacts, résidence par
tiers, frontières de provenance, télémétrie expurgée et API asynchrone
`AiRuntime::resolve`. Elle ne dépend d'aucun framework ML.
La normalisation lightweight des espaces Unicode construit seulement sa
`String` de sortie bornée, sans retenir une liste intermédiaire de tous les mots.

La release beta fournit aussi batching adaptatif au backend, batch Candle
vectorisé, coordination LRU bornée des loads et `ModelLoadSnapshot` public.
Les artefacts locaux utilisent ouverture no-follow, revalidation du handle et
activation atomique sans remplacement. Registres, routes apprises, résidence,
loads et claims Swarm ont des limites fixes.
Une activation locale idempotente ou une course entre writers revalide et
compare l'artefact existant incrémentalement avec un buffer fixe de 16 Kio ; un
second artefact complet n'est jamais chargé à côté des octets du caller.
`ArtifactStore::load_lease` permet au backend d'utiliser les octets vérifiés
pendant le décodage : les tiers mémoire partagent leur `Arc<[u8]>` résident,
tandis que fichier et peer gardent leur allocation owned. Un lease mémoire
actif reste comptabilisé et empêche l'éviction jusqu'à sa libération.
`ModelRegistry::get_lease` et `candidate_leases` renvoient aussi des snapshots
modèle immuables partagés. Le router emploie directement ces leases : la
découverte ne clone plus les descriptors complets avant de les cloner encore
dans les routes locales. `get` et `candidates` restent des adaptateurs owned
compatibles ; les mutations utilisent copy-on-write et ne gardent jamais le
lock du registre pendant le travail backend.
`ModelRegistryLimits` borne aussi les modèles, les localisations par modèle,
leur total et leurs octets comptabilisés. Les plafonds par défaut sont 4 096
modèles, 128 localisations par modèle, 65 536 localisations et 8 Mio de metadata
de localisation ; les callers peuvent choisir des limites plus strictes. Les
itérateurs initiaux et ajouts ultérieurs échouent avant rétention, les doublons
restent idempotents sans copy-on-write et `ModelRegistry::pressure` expose les
comptes/octets courants et de pic ainsi que les rejets.
Le loader Candle transfère labels, poids et biais décodés dans le modèle chargé
sans cloner ces buffers complets. `CandleBackend` réserve un slot et les octets
déclarés de l'artefact avant lecture ou décodage ;
`new_with_loaded_byte_limit` réduit le plafond agrégé et `memory_pressure`
expose usage courant/de pic et rejets anticipés. La réservation suit les leases
d'inférence actifs après `unload`. Ce backend est un classificateur sans KV
cache génératif ; les moteurs génératifs externes doivent borner leur cache.
Les bodies encodés OpenAI-compatible réutilisent la même allocation immuable
partagée dans la requête transport, le transfert au worker blocking et le
`HttpRequest` bas niveau. Deux copies intégrales possibles disparaissent sans
modifier le trait transport emprunté ni l'annulation bornée.

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

Détecter un GPU ne signifie pas exécuter une inférence GPU. Candle reste CPU-only
même avec `accelerator-nvidia` ; les deux adaptateurs rejettent les IDs de
dispositif non enregistrés, et l'adaptateur HTTP le fait avant encodage ou envoi.
Le lien avec un dispositif physique externe appartient au deployment, pas au
protocole chat. Voir la [matrice d'exécution](wiki/resources.fr.md#matrice-dexécution--détection-ne-signifie-pas-inférence).

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
Le décodeur SSE analyse les frames complets coalescés directement depuis le
chunk emprunté, ne retient qu'une queue incomplète et compacte les octets en
attente une fois par chunk.

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

L'intégration de déploiement peut composer un flux explicite via Supervisor et
`CapabilityRegistry` sans modifier V1. La sélection
déclarative reste un travail post-1.0 hors de la portée beta. Voir le
[rapport release](wiki/release-readiness.fr.md) et le
[threat model](wiki/threat-model.fr.md).

## Documentation stable

Identifiant stable : **ACR-022**. Consultez le
[guide complémentaire d’architecture et d’intégration](https://wiki.appcore.dnettoraw.com/fr/crates/id/acr-022). Cet identifiant
permanent reste valable si la page du wiki est déplacée.
