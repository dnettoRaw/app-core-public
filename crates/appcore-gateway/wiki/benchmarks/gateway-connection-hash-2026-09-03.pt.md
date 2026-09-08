# Benchmark do hash de conexão do Gateway — 2026-09-03

O workload de processo `appcore-dev bench --name appcore-gateway` rodou em Apple
M1 macOS/aarch64 com Rust 1.97.1. Cada resultado usa cinco amostras release
calibradas após um warmup descartado. O caso worker vincula 64 capabilities
distintas de 128 bytes, o formato máximo aceito na conexão.

| Caso | p50 anterior | p50 final | p95 anterior | p95 final | Mudança |
|---|---:|---:|---:|---:|---:|
| Hash da conexão client | 1,600 us | 1,288 us | 1,638 us | 1,326 us | p50 -19,54% |
| Hash da conexão worker | 115,765 us | 106,769 us | 119,794 us | 108,556 us | p50 -7,77% |

O delta de RSS da workload worker caiu de 0,34 para 0,27 MiB (-22,73%) e o
delta retido de 0,33 para 0,27 MiB (-19,05%). O delta da workload client caiu
de 0,19 para 0,09 MiB. O framing máximo não mantém mais seu frame binário de
8,5 KiB nem uma segunda string de payload de 17 KiB ao lado do output
hexadecimal final exigido.

O comparador aprovou os seis casos Gateway, controles e alterados. Estas são
medições locais do repositório, não claims de produção multiplataforma.
