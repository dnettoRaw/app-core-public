# Benchmark de checkpoint incremental — 2026-09-03

O workload de processo `appcore-dev bench --name appcore-sync` rodou em Apple
M1 macOS/aarch64 com Rust 1.97.1. Cada resultado usa cinco amostras release
calibradas após um warmup descartado. O fixture possui 32.768 IDs de peer
distintos com 128 bytes e busca o último peer enquanto valida todos os records
V1.

| Medição | Antes | Incremental | Variação |
|---|---:|---:|---:|
| p50 | 21,566 ms | 17,305 ms | -19,76% |
| p95 | 21,731 ms | 17,418 ms | -19,85% |
| RSS pico | 21,50 MiB | 5,67 MiB | -73,62% |
| Delta RSS da carga | 16,08 MiB | 0,25 MiB | -98,45% |
| Delta RSS retido | 16,08 MiB | 0,25 MiB | -98,45% |

Startup e lookup agora percorrem o arquivo com reader fixo de 16 KiB e não
retêm um mapa completo de peers. A mutação ainda possui um mapa canônico
ordenado para preservar duplicatas e ordenação V1, mas o grava diretamente em
vez de montar outra string do tamanho do arquivo. Tetos de arquivo, linha e
records falham fechados.

Estas são medições locais do repositório, não claims de produção
multiplataforma.
