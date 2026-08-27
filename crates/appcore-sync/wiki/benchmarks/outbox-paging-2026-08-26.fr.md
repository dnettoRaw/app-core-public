# Benchmark de pagination SyncOutbox — 2026-08-26

Commit d'implémentation : `c904e833c4cf973b5a9b91916119935c0bcb5da8`

L'exécution propre release-profile de `appcore-dev cert bottlenecks` utilisait
Rust 1.97.1 sur macOS/aarch64. Elle a placé 256 messages avec des fixtures
d'événement de 32 Kio dans la file et mesuré 16 snapshots complets face à 16
pages bornées à huit messages et 512 Kio.

| Lecture | Messages retournés | Octets matérialisés | p99 | Débit |
|---|---:|---:|---:|---:|
| Snapshot complet de compatibilité | 256 | 30 021 820 | 1 404 417 ns | 1 627/s |
| Page bornée | 7 | 460 684 | 71 458 ns | 15 754/s |
| Stats sans payload | 0 | 0 octet de payload | 54 542 ns | 19 258/s |

La page s'est arrêtée à la limite d'octets avant de cloner le huitième message
et a réduit les octets matérialisés de 98,46 %. Un timestamp readiness futur a
masqué la tête ordonnée et un receipt exact de quatre messages n'a supprimé que
ce préfixe. Tous les sous-systèmes ont réussi et le processus complet a atteint
un pic de 244 752 Kio.

Il s'agit d'une preuve locale au dépôt, pas d'une charge de production ni d'une
certification multiplateforme. Les fixtures du wire V1 restent inchangées.
