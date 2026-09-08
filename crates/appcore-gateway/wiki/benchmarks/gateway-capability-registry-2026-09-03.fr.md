# Benchmark du registre de capabilities Gateway — 2026-09-03

L'exécution release de `appcore-dev bench --name appcore-gateway` utilisait Rust
1.97.1 sur macOS/aarch64 avec Apple M1. Chaque cas isolé conservait 1 024
workers annonçant le maximum de 64 noms de capability partagés. Elle utilisait
un warmup, cinq processus mesurés et une calibration automatique vers 20 ms.

| Index inverse | Lookup p50 | Lookup p95 | Pic RSS | RSS après teardown |
|---|---:|---:|---:|---:|
| Nom owned par worker | 55,53 ns | 59,59 ns | 30,20 Mio | 30,12 Mio |
| Owner de nom partagé | 50,22 ns | 53,20 ns | 27,70 Mio | 24,70 Mio |

Le registre partagé a réduit le pic RSS de 8,28%, le RSS après teardown de
18,00% et le lookup p50 de 9,56%. Le delta RSS de la workload est passé de
24,75 à 22,25 Mio. L'implémentation conserve une `HashMap` directe
capability→workers sur le hot path ; l'enregistrement retrouve et partage le
nom immuable déjà possédé.

Les tests prouvent aussi la normalisation des annonces dupliquées, le partage
des noms immuables entre clones du registre, l'indépendance des index après
deregistration et la libération de tous les owners après le départ du dernier
annonceur. Ces mesures sont une preuve locale, pas une certification de
production cross-platform.
