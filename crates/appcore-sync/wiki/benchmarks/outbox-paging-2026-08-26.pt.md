# Benchmark de paginação do SyncOutbox — 2026-08-26

Commit da implementação: `c904e833c4cf973b5a9b91916119935c0bcb5da8`

A execução limpa release-profile de `appcore-dev cert bottlenecks` usou Rust
1.97.1 em macOS/aarch64. Ela enfileirou 256 mensagens com fixtures de evento de
32 KiB e mediu 16 snapshots completos contra 16 páginas limitadas a oito
mensagens e 512 KiB.

| Leitura | Mensagens retornadas | Bytes materializados | p99 | Throughput |
|---|---:|---:|---:|---:|
| Snapshot completo de compatibilidade | 256 | 30.021.820 | 1.404.417 ns | 1.627/s |
| Página limitada | 7 | 460.684 | 71.458 ns | 15.754/s |
| Stats sem payload | 0 | 0 bytes de payload | 54.542 ns | 19.258/s |

A página parou no limite de bytes antes de clonar a oitava mensagem e reduziu
os bytes materializados em 98,46%. Um timestamp futuro de readiness ocultou a
frente ordenada, e um receipt exato de quatro mensagens removeu somente esse
prefixo. Todos os subsistemas passaram e o processo completo atingiu pico de
244.752 KiB.

Isto é evidência local do repositório, não workload de produção nem
certificação multiplataforma. As fixtures do wire V1 permanecem inalteradas.
