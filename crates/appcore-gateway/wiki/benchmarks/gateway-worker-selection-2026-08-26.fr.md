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

## Suivi avec candidats empruntés — 2026-09-03

Le workload processus `worker_selection_round_robin_1024` exerce le plafond de
workers par tenant. Cinq échantillons release calibrés, après un warmup, ont
réduit le p50 de 468,86 à 335,56 us (-28,43 %) et le p95 de 471,98 à 339,67 us
(-28,03 %). Le slot de métadonnées candidat est passé de 104 à 40 octets sur
macOS/aarch64, sans compter les chaînes d'identifiants possédées supprimées. Le
delta RSS retenu a baissé de 4,46 % ; le pic RSS a varié de +1,05 %, dans la
marge de bruit du comparateur. Les policies sans buffer parcourent désormais
sans `Vec` candidat.

## Suivi du nombre d'allocations — 2026-09-03

L'allocator de certification a révélé un autre coût de lookup : chaque candidat
créait un tuple owned `(installation_id, core_id)` uniquement pour interroger la
map des workers. L'index Core existant devient le fast path sans allocation,
avec un scan exact limité à 1 024 workers lorsque des installations partagent un
Core ID. Sur des exécutions Gateway complètes équivalentes, les allocations sont
passées de 7 439 239 à 810 640 (-89,10 %) et les octets demandés de 161 250 071
à 88 341 495 (-45,21 %). Le p99 est passé de 19 334 à 16 250 ns en round-robin,
de 8 000 à 6 750 ns en least-inflight et de 31 542 à 24 958 ns en affinity.
