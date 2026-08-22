# Release beta : 0.1.0-beta.1

[English](release-readiness.en.md) | [Português](release-readiness.pt.md) |
[Performance](benchmarks.fr.md) | [Threat model](threat-model.fr.md) |
[LLM génératifs](generative-llm.fr.md)

Décision du 2026-08-22 : publier `0.1.0-beta.1` dans la frontière de support
ci-dessous. La publication doit provenir du commit de release propre, et le tag
immuable est créé seulement après vérification du package dans le registry.

La portée beta couvre le cœur local borné, ResourceGovernor, CostScheduler,
admission, batching, résidence, artefacts vérifiés, resolver lightweight et les
adapters Candle/OpenAI-compatible activés explicitement. Elle ne certifie pas
chaque engine ou accélérateur accepté par ces adapters. `swarm` et
`appcore-bin/ai-alpha` restent des surfaces d'intégration expérimentales.

## Preuves produites

- `perf_lab` déterministe mesure resolve, scaling registre/scheduler, batching,
  résidence, artefacts, Candle/training et Swarm avec sortie JSONL ;
- la lecture chaude du snapshot ressources atteint 167 ns p50 sur l'hôte de
  référence ; sampling dynamique forcé 2,416 us et discovery statique 2,833 us ;
- la revalidation ne clone plus les payloads ; routing évite scans répétés et
  récupération quadratique, avec metadata modèle immuable partagée ;
- sampling matériel et load modèle sont single-flight ; files et batches
  respectent annulation, deadline, mémoire, latence et plafond backend ;
- macOS CPU/RAM et mémoire unifiée Apple ont été exécutés sur l'hôte de
  référence ; Linux/Windows et NVIDIA NVML optionnel font le cross-compile ;
- les artefacts utilisent open no-follow, revalidation du handle et activation
  atomique ; 32 writers laissent un seul fichier vérifié ;
- Candle vectorise les batches et les borne à 64 avec résultat par item ;
- Swarm rejette replay stale, claims dupliqués et metadata incohérente ou
  excessive ; peers, transferts et routes apprises sont bornés ;
- le soak a traité 100 000 requêtes exactes sans état bloqué ; les trois cibles
  fuzz compilent ;
- `default = []` reste ; NVIDIA, Candle, HTTP, training et Swarm sont opt-in.

Le [rapport performance](benchmarks.fr.md) donne tout l'avant/après, y compris
les régressions des petits batches et le coût voulu des ranges sécurisés. Le
[threat model](threat-model.fr.md) décrit les risques résiduels.

## Matrice d'entrée beta

| Exigence | État beta | Preuve ou frontière |
|---|---|---|
| API default légère et `resolve` mesuré | PASS | aucun ML/HTTP default ; benchmark déterministe |
| governor, scheduler, files et batches bornés | PASS | tables, contention, annulation, deadline et single-flight |
| placement par ressources | PASS | device exact, mémoire unifiée, hysteresis et budgets par mode |
| CPU/RAM et GPU Apple unifié | PASS sur macOS arm64 de référence | sortie réelle `hardware_report` |
| probes Linux/Windows | IMPLÉMENTÉ, NON CERTIFIÉ PHYSIQUEMENT | cross-compile ; validation demandée aux beta testers |
| NVIDIA/AMD/NPU | PARTIEL | NVML et DRM Linux implémentés ; NPU indisponible, jamais simulé |
| intégrité/race artefact | PASS sur Unix de référence | no-follow, revalidation et 32 writers |
| recovery load et stress | PASS | 100 loads concurrents et soak de 100 000 requêtes |
| Candle/training/OpenAI optionnels | PASS local | features, decoding borné, batch 1/8/32 et rejet au-dessus de 64 |
| API, dépendances et features | PASS | exports classés, isolation features et metadata package |
| sécurité et supply chain | PASS AVEC WARNING ACCEPTÉ | aucune vulnérabilité connue ; Candle optionnel apporte `paste` non maintenu via `gemm` |
| Swarm | EXPÉRIMENTAL | planner/validation local passe ; aucun adapter Peer RPC production annoncé |
| isolation engine externe | PROPRIÉTÉ DU DÉPLOIEMENT | Candle est in-process ; la politique processus/sandbox externe n'appartient pas à la crate |
| composition déclarative V1 | HORS PÉRIMÈTRE | V1 est gelé ; composition Rust explicite pour la beta |

## Limites délibérées de la beta

Streaming tokens, engine PDF/OCR intégré, downloads automatiques, gestion du
processus engine, probe NPU, streaming reprenable d'artefacts entre peers et
transport Swarm production ne sont ni implémentés ni annoncés. Une métrique
inconnue reste inconnue. La certification d'accélérateurs multiplateformes et
le soak prolongé d'un modèle réel sont des preuves du programme beta, pas des
passes locales inventées.

Résultat : **READY FOR BETA** dans ce périmètre. La procédure est un commit
propre, preflight registry sans upload, upload confirmé, vérification du package
dans le registry puis seulement création du tag immuable.
