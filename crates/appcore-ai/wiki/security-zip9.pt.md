# Distribuição de segurança ZIP do Candle

`appcore-ai 1.0.0-rc.2` seleciona `dnet-candle-core =1.0.0-rc.1` e `dnet-candle-nn =1.0.0-rc.1` do crates.io para o backend Candle opcional. São distribuições de segurança derivadas do Hugging Face Candle 0.11.0, não releases oficiais upstream nem novos módulos Runtime.

O núcleo oficial 0.11.0 exige ZIP 8.6.x, afetado por RUSTSEC-2026-0367, RUSTSEC-2026-0368 e RUSTSEC-2026-0369. O núcleo corrigido fixa `zip =9.0.4`; NN seleciona a mesma identidade de núcleo. Três chamadas de nomes NPZ/PTH propagam a API falível do ZIP 9. As operações tensor/NN são preservadas. SHA-256 dos arquivos originais, revisão Git, diferenças exatas de código e licenças estão registados no repositório da distribuição. Não há exclusão de alertas, patch local oculto ou dependência Git distribuída.

`backend-candle`, `training-candle`, formato `NativeLinearV1`, IDs de backend e campos Rust públicos mantêm-se. O build padrão continua sem dependência de framework ML. As aplicações usam os contratos de dados e a fachada AppCore existentes; nenhum tensor Candle é acrescentado à fachada pública. Validar CPU não certifica CUDA/Metal/MKL.

As regressões cobrem ida e volta NPZ Unicode, nomes antigos CP437 e erros controlados de arquivos malformados pelos leitores NPZ imediato/lazy e PTH. Os testes CPU upstream e os testes AppCore AI com todas as features são executados separadamente. ZIP 9 conserva a descodificação CP437; bytes UTF-8 inválidos não são automaticamente rejeitados como erro de nome.

A adoção fixa explicitamente `appcore-ai = "=1.0.0-rc.2"` após qualificação no registry e atualiza o lockfile, incluindo features opcionais. Conferir `cargo tree --all-features -i zip` e `cargo audit`. Pins exatos existentes não atualizam sozinhos. Restaurar o grafo anterior completo exige avaliar as vulnerabilidades conhecidas do ZIP 8. Preferir uma release oficial Candle qualificada quando o grafo publicado suportar ZIP >=9; não misturar identidades de núcleo silenciosamente.

[Source and provenance](https://github.com/dnettoRaw/candle-zip9-security)
