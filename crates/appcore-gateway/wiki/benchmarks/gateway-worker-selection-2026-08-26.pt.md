# Benchmark de seleção de workers do Gateway — 2026-08-26

Commit da implementação: `8e77c99f18dfee6373e7fe9e0c14aeb5fdd81e39`

A execução limpa release-profile de `appcore-dev cert bottlenecks` usou Rust
1.97.1 em macOS/aarch64. Ela registrou 64 workers para uma capability de um
tenant e executou 16.384 seleções por policy medida.

| Policy | p50 | p95 | p99 | Máximo | Throughput | Budget |
|---|---:|---:|---:|---:|---:|---:|
| Round-robin | 13.333 ns | 14.958 ns | 17.250 ns | 134.459 ns | 73.341/s | p99 <= 1 ms; >= 10.000/s |
| Least-inflight | 13.750 ns | 14.500 ns | 15.666 ns | 79.583 ns | 71.599/s | p99 <= 1 ms; >= 10.000/s |
| Affinity stateless | 28.709 ns | 30.416 ns | 33.666 ns | 180.500 ns | 34.361/s | p99 <= 1 ms; >= 10.000/s |

Cada um dos 64 workers recebeu exatamente quatro requests na verificação de
distribuição round-robin. As invariantes de health weighting, rejeição por
fila/capacidade e affinity stateless estável passaram. O resolver ocupou 16
bytes e o processo completo entre subsistemas atingiu pico de 264.560 KiB sob
o teto de 786.432 KiB.

Isto é evidência de performance local do repositório, não workload de produção
ou certificação multiplataforma. As chaves de affinity e identidades de worker
são valores da fixture e não são mantidas pela telemetria.
