# Evidência do preflight de capacidades storage V1 — 2026-08-26

[English](storage-capability-v1-2026-08-26.en.md) |
[Français](storage-capability-v1-2026-08-26.fr.md) |
[Guia](../guide.pt.md)

A certificação release-profile clean-source passou em macOS/aarch64 com Rust
1.97.1 no commit `12cbfc32264a57eb19b7e5c9e36ce076b3a1aee6`.

| Observação | Resultado | Gate |
|---|---:|---:|
| Tipos de capacidade | 7 | exato |
| Capacidade do catálogo | 32 | exato |
| Iterações de preflight | 16.384 | exato |
| p50 | 42 ns | registrado |
| p95 | 42 ns | registrado |
| p99 | 83 ns | ≤ 1.000.000 ns |
| Throughput | 10.493.879 ops/s | ≥ 10.000 ops/s |
| Requisito não suportado | falhou fechado | obrigatório |
| Pico RSS da suíte | 320.464 KiB | ≤ 786.432 KiB |

Não existe latência anterior porque o host anterior não executava preflight de
capacidades de storage. O baseline era ausência de validação, não uma operação
equivalente mais rápida.

```bash
cargo run --release -p appcore-certification -- \
  bottlenecks builds/certification/ac016-bottlenecks.json
```

Esta evidência certifica o contrato de desenvolvimento pós-1.0. Ela não altera
nem republica manifests V1 congelados.
