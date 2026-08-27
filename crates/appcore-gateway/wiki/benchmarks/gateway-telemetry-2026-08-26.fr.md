# Benchmark de la télémétrie Gateway — 2026-08-26

Commit d'implémentation : `31c4fbec34d403770bf59dfe76d36732cb9b4450`

L'exécution propre release-profile de `appcore-dev cert bottlenecks` utilisait
Rust 1.97.1 sur macOS/aarch64. Elle a conservé 128 séries de capability, agrégé
huit noms supplémentaires dans une série d'overflow fixe, exécuté 4 096 routes
sans worker disponible et construit 256 snapshots. L'inflight résiduel était
nul et le rapport de certification complet a réussi.

| Mesure | p50 | p95 | p99 | Maximum | Débit | Budget |
|---|---:|---:|---:|---:|---:|---:|
| Rejet de route instrumenté | 1 666 ns | 1 709 ns | 1 792 ns | 10 125 ns | 591 067/s | p99 <= 1 ms |
| Snapshot de 129 séries | 5 417 ns | 5 708 ns | 5 792 ns | 14 084 ns | 181 281/s | p99 <= 5 ms |

Il s'agit d'une preuve de performance locale au dépôt, pas d'une certification
de trafic ou collector en production. Les adapters Prometheus/OpenTelemetry,
leurs queues et leurs échecs réseau appartiennent au déploiement.
