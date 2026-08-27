# Benchmark da telemetria Gateway — 2026-08-26

Commit da implementação: `31c4fbec34d403770bf59dfe76d36732cb9b4450`

A execução limpa release-profile de `appcore-dev cert bottlenecks` usou Rust
1.97.1 em macOS/aarch64. Ela manteve 128 séries de capability, agregou oito
nomes adicionais em uma série fixa de overflow, executou 4.096 rotas sem worker
disponível e construiu 256 snapshots. O inflight residual foi zero e o relatório
completo de certificação passou.

| Medição | p50 | p95 | p99 | Máximo | Throughput | Budget |
|---|---:|---:|---:|---:|---:|---:|
| Rejeição de rota instrumentada | 1.666 ns | 1.709 ns | 1.792 ns | 10.125 ns | 591.067/s | p99 <= 1 ms |
| Snapshot de 129 séries | 5.417 ns | 5.708 ns | 5.792 ns | 14.084 ns | 181.281/s | p99 <= 5 ms |

Isto é evidência de performance local do repositório, não certificação de
tráfego ou collector em produção. Adapters Prometheus/OpenTelemetry, suas filas
e suas falhas de rede pertencem ao deployment.
