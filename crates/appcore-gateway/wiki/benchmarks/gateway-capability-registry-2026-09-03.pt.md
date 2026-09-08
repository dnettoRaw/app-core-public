# Benchmark do registry de capabilities do Gateway — 2026-09-03

A execução release de `appcore-dev bench --name appcore-gateway` usou Rust
1.97.1 em macOS/aarch64 com Apple M1. Cada caso isolado manteve 1.024 workers
anunciando o máximo de 64 nomes compartilhados de capability. Foram usados um
warmup, cinco processos medidos e calibração automática para 20 ms.

| Índice reverso | Lookup p50 | Lookup p95 | RSS pico | RSS após teardown |
|---|---:|---:|---:|---:|
| Nome owned por worker | 55,53 ns | 59,59 ns | 30,20 MiB | 30,12 MiB |
| Owner de nome compartilhado | 50,22 ns | 53,20 ns | 27,70 MiB | 24,70 MiB |

O registry compartilhado reduziu o RSS pico em 8,28%, o RSS após teardown em
18,00% e o lookup p50 em 9,56%. O delta de RSS da workload caiu de 24,75 para
22,25 MiB. A implementação mantém um `HashMap` direto capability→workers no hot
path; o registro encontra e compartilha o nome imutável já possuído.

Os testes também provam normalização de anúncios duplicados, compartilhamento
dos nomes imutáveis entre clones do registry, independência dos índices após
deregistration e liberação de todos os owners ao remover o último anunciante.
As medições são evidência local do repositório, não certificação cross-platform.
