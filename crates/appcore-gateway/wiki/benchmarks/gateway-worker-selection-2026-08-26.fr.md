# Benchmark de sélection des workers Gateway — 2026-08-26

Commit d'implémentation : `8e77c99f18dfee6373e7fe9e0c14aeb5fdd81e39`

L'exécution propre release-profile de `appcore-dev cert bottlenecks` utilisait
Rust 1.97.1 sur macOS/aarch64. Elle a enregistré 64 workers pour une capability
d'un tenant et exécuté 16 384 sélections par policy mesurée.

| Policy | p50 | p95 | p99 | Maximum | Débit | Budget |
|---|---:|---:|---:|---:|---:|---:|
| Round-robin | 13 333 ns | 14 958 ns | 17 250 ns | 134 459 ns | 73 341/s | p99 <= 1 ms ; >= 10 000/s |
| Least-inflight | 13 750 ns | 14 500 ns | 15 666 ns | 79 583 ns | 71 599/s | p99 <= 1 ms ; >= 10 000/s |
| Affinity stateless | 28 709 ns | 30 416 ns | 33 666 ns | 180 500 ns | 34 361/s | p99 <= 1 ms ; >= 10 000/s |

Chacun des 64 workers a reçu exactement quatre requests dans la vérification
de distribution round-robin. Les invariants health weighting, rejet par
file/capacité et affinity stateless stable ont réussi. Le resolver occupait 16
octets et le processus inter-sous-systèmes complet a culminé à 264 560 Kio sous
son plafond de 786 432 Kio.

Il s'agit d'une preuve de performance locale au dépôt, pas d'un workload de
production ni d'une certification multiplateforme. Les clés affinity et les
identités worker sont des valeurs de fixture non conservées par la télémétrie.
