# Release beta: 0.1.0-beta.1

[English](release-readiness.en.md) | [Français](release-readiness.fr.md) |
[Performance](benchmarks.pt.md) | [Threat model](threat-model.pt.md) |
[LLMs generativos](generative-llm.pt.md)

Decisão em 2026-08-22: lançar `0.1.0-beta.1` dentro da fronteira de suporte
abaixo. A publicação deve partir do commit de release limpo, e a tag imutável
somente é criada depois de verificar o pacote no registry.

A claim beta cobre o core local limitado de orquestração, ResourceGovernor,
CostScheduler, admission, batching, residency, artefatos verificados, resolver
lightweight e os adapters Candle e OpenAI-compatible ativados explicitamente.
Ela não certifica todo engine ou acelerador aceito por esses adapters. `swarm` e
`appcore-bin/ai-alpha` continuam superfícies experimentais de integração.

## Evidências produzidas

- `perf_lab` determinístico mede resolve, scaling de registry/scheduler,
  batching, residency, artefatos, Candle/training e Swarm, com saída JSONL;
- leitura quente do snapshot de recursos chegou a 167 ns p50 no host de
  referência; sampling dinâmico forçado chegou a 2,416 us e discovery estático
  a 2,833 us p50;
- a revalidação não clona payloads; routing evita scans repetidos e recuperação
  quadrática, compartilhando metadata imutável de modelo;
- sampling de hardware e load de modelo são single-flight; filas rejeitam jobs
  cancelados/expirados e batches respeitam latência, memória e teto do backend;
- detecção nativa macOS de CPU/RAM e memória unificada Apple executou no host de
  referência; probes Linux/Windows e NVIDIA NVML opcional fazem cross-compile;
- artefatos usam open no-follow, revalidação do handle e ativação atômica; uma
  corrida de 32 writers deixa um único arquivo verificado;
- batches Candle são vetorizados e limitados a 64, com resultado por item;
- anúncios Swarm rejeitam replay stale, claims duplicadas e metadata excessiva
  ou inconsistente; peers, transfers e rotas aprendidas são limitados;
- o soak de certificação processou 100.000 requests exatos sem estado preso de
  fila ou load; os três fuzz targets compilam;
- `default = []` permanece; NVIDIA, Candle, HTTP, training e Swarm são opt-in.

O [relatório de performance](benchmarks.pt.md) registra o antes/depois completo,
inclusive regressões de batch pequeno e o custo deliberado da leitura segura de
ranges. O [threat model](threat-model.pt.md) registra os riscos residuais.

## Matriz de entrada na beta

| Requisito | Estado beta | Evidência ou fronteira |
|---|---|---|
| API default leve e `resolve` medido | PASS | sem ML/HTTP default; benchmark determinístico |
| governor, scheduler, filas e batches limitados | PASS | tabelas, contenção, cancelamento, deadline e single-flight |
| placement orientado a recursos | PASS | device exato, memória unificada, hysteresis e budgets por modo |
| CPU/RAM e GPU Apple unificada | PASS no macOS arm64 de referência | saída real do `hardware_report` |
| probes Linux/Windows | IMPLEMENTADO, NÃO CERTIFICADO FISICAMENTE | cross-compile; testers beta devem validar no hardware |
| NVIDIA/AMD/NPU | PARCIAL | NVML e DRM Linux implementados; NPU fica indisponível, sem simulação |
| integridade e corrida de artefatos | PASS no Unix de referência | no-follow, revalidação e 32 writers |
| recovery de load e stress | PASS | 100 requests concorrentes de load e soak de 100.000 requests |
| Candle/training/OpenAI opcionais | PASS local | features, decoding limitado, batch 1/8/32 e rejeição acima de 64 |
| API, dependências e features | PASS | exports classificados, isolamento de features e metadata do pacote |
| segurança e supply chain | PASS COM WARNING ACEITO | nenhuma vulnerabilidade conhecida; Candle opcional traz `paste` sem manutenção via `gemm` |
| Swarm | EXPERIMENTAL | planner/validação local passa; adapter Peer RPC de produção não é anunciado |
| isolamento de engine externo | PERTENCE AO DEPLOYMENT | Candle é in-process; política de processo/sandbox externo não pertence à crate |
| composição declarativa V1 | FORA DO ESCOPO | V1 está congelado; composição Rust explícita é o caminho beta suportado |

## Limitações deliberadas da beta

Streaming de tokens, engine PDF/OCR embutido, downloads automáticos, gestão de
processo do engine, probe NPU, streaming retomável de artefatos entre peers e
transporte Swarm de produção não estão implementados nem são anunciados. Métrica
desconhecida permanece desconhecida. Certificação de aceleradores em outras
plataformas e soak prolongado com modelo real são evidências do programa beta,
não passes locais inventados.

Resultado: **READY FOR BETA** dentro do escopo acima. O procedimento é commit
limpo, preflight de registry sem upload, upload confirmado, verificação do
pacote no registry e somente então criação da tag imutável.
