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

## Follow-up com candidatos emprestados — 2026-09-03

O workload de processo `worker_selection_round_robin_1024` exercita o teto de
workers por tenant. Cinco amostras release calibradas, após um warmup, reduziram
o p50 de 468,86 para 335,56 us (-28,43%) e o p95 de 471,98 para 339,67 us
(-28,03%). O slot de metadados do candidato caiu de 104 para 40 bytes em
macOS/aarch64, sem contar as strings owned de identificadores eliminadas. O
delta de RSS retido caiu 4,46%; o RSS pico variou +1,05%, dentro da margem de
ruído do comparador. Policies sem buffer agora percorrem sem um `Vec` candidato.

## Follow-up de contagem de alocações — 2026-09-03

O allocator de certificação expôs outro custo de lookup: cada candidato criava
uma tuple owned `(installation_id, core_id)` apenas para consultar o mapa de
workers. O índice existente por Core agora é o fast path sem alocação, com scan
exato limitado a 1.024 workers quando instalações compartilham um Core ID. Em
execuções equivalentes do Gateway completo, allocs caíram de 7.439.239 para
810.640 (-89,10%) e bytes solicitados de 161.250.071 para 88.341.495 (-45,21%).
O p99 caiu de 19.334 para 16.250 ns no round-robin, 8.000 para 6.750 ns no
least-inflight e 31.542 para 24.958 ns no affinity.
