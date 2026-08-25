# Guia do appcore-ai

[English](guide.en.md) | [Français](guide.fr.md) |
[Exemplo básico](examples/basic.pt.md) |
[Exemplo intermediário](examples/intermediate.pt.md) |
[Receitas concretas](recipes.pt.md) |
[Modelos e training](models.pt.md) |
[LLMs generativos](generative-llm.pt.md) |
[Recursos de hardware](resources.pt.md) |
[ADR](architecture-adr.pt.md) | [Threat model](threat-model.pt.md) |
[Prontidão de release](release-readiness.pt.md)

`appcore-ai` possui a orquestração de IA genérica e limitada. Prompts,
schemas, credenciais e workflows de negócio pertencem às aplicações. A crate
tem SemVer independente; esta release é `0.1.0-beta.2`.

## Rota de aprendizado

1. Execute o [runtime lightweight](examples/basic.pt.md) sem features opcionais.
2. Execute a [inferência Candle via AiRuntime](examples/intermediate.pt.md).
3. Configure ou treine um classificador em [modelos e training](models.pt.md).
4. Leia o [runtime adaptativo](generative-llm.pt.md) para texto, visão, PDF,
   seleção multi-engine, residência própria e modelos aconselháveis.
5. Use as [receitas concretas](recipes.pt.md) para recursos, cache, cancelamento,
   Swarm, training, observabilidade e backpressure.
6. Antes de produção, leia o [threat model](threat-model.pt.md) e a
   [prontidão de release](release-readiness.pt.md).

Os exemplos `lightweight_runtime`, `candle_runtime`, `openai_compatible` e
`candle_training` são
fontes compilados no diretório `examples/`; os blocos principais da wiki são
derivados deles e incluem comandos e saídas esperadas.

## Arquitetura e resolução

```text
AiRuntime::resolve
  -> valida modalidade, conteúdo, privacidade, autorização e limites
  -> resolver lightweight determinístico
  -> filtra o piso Fast/Balanced/Deep/Maximum
  -> registry de modelo e identidade do artefato
  -> admission pelo ResourceGovernor
  -> scheduler de custo (CPU/GPU/NPU/peer autorizado)
  -> admissão de execução justa e limitada
  -> `infer` ou `infer_batch` compatível coordenado explicitamente
  -> residency (VRAM -> RAM -> local -> peer)
  -> backend ou SwarmBridge
  -> escalation limitada
  -> diagnóstico e telemetria redigidos
```

O scheduler pondera carga, headroom, profundidade de fila, residency, custo de
load/transfer, EMA de latência/throughput, prioridade, deadline e modo de
recursos. Pesos inteiros e clocks injetáveis mantêm testes determinísticos.
Compute e storage são decisões separadas.

O caminho lightweight normaliza texto e aplica regras exact/prefix/contains
limitadas. Ele declara motivo e certeza, podendo responder ou guardar um
fallback seguro antes da escalation.

## Recursos e modos

`ResourceGovernor` usa `HardwareProbe`, cache e hysteresis. RAM/VRAM desconhecida
não vira capacidade infinita. Budgets locais e doados são distintos;
`AiContributionPolicy` desliga compute e storage de forma independente.

`SystemHardwareProbe::default()` lê CPU/RAM reais no macOS, Linux e Windows.
Ele descobre GPU Apple com memória unificada, devices DRM no Linux e, com
`accelerator-nvidia`, VRAM/utilização NVIDIA via NVML. Fit por device exato
impede somar GPUs independentes. A [página de recursos](resources.pt.md) traz
matriz, relatório executável, custo da dependência e semântica operacional.

| Modo | Política voluntária do AppCore |
|---|---|
| `Eco` | maior headroom, batches mínimos |
| `Balanced` | preserva interatividade e reduz batch de training |
| `Performance` | favorece throughput com margem de segurança |
| `Unrestricted` | remove headroom voluntário dentro dos limites de backend/SO |
| `Custom` | aplica tetos explícitos validados |

`Unrestricted` não desliga proteções de SO, driver, firmware, temperatura ou
energia e não garante ausência de throttling. Filas, batches, tentativas,
peers, artefatos, transfers, inputs, outputs, workers e jobs são limitados.

## Modelos, artefatos e residency

`ModelRegistry` separa metadata, lifecycle e localizações. `ArtifactIdentity`
usa SHA-256, tamanho exato e publisher opcional. Cache local usa arquivo
temporário exclusivo, sync e ativação atômica; nomes recebidos de peers não são
confiados.

```text
ArtifactIdentity -> Vram(device) | Memory | LocalStorage | Peer(peer)
```

`ResidencyPlanner` oferece reuso LRU simples, eviction em duas fases, prefetch
limitado, fallbacks e rollback. Requests concorrentes observam `InFlight` em
vez de carregar o mesmo target duas vezes.

## Backend e training opcionais

O default não contém framework ML. `backend-candle` habilita inferência CPU
real para o formato data-only `NativeLinearV1`: load verificado, unload,
inferência concorrente, cancelamento e métricas, sem download automático.

```bash
cargo run -p appcore-ai --example candle_cpu --features backend-candle
```

`training-candle` adiciona SGD local reprodutível. Dataset, dimensões, labels,
epochs, steps, batch, recursos e checkpoints são limitados; checkpoints usam
ativação atômica e suportam resume. Training distribuído não é suportado.

`backend-openai-compatible` é o caminho generativo real para servidor
llama.cpp, MLX-LM, TabbyAPI, vLLM, SGLang, TensorRT-LLM, OpenVINO ou compatível
testado, executado separadamente. Ele suporta chat com papéis, sampling
limitado, tools/tool calls e imagem declarada. O transporte default é
loopback-first e sem autenticação; credencial remota exige adapter apoiado por
AppCore security.

## Local, Swarm e Auto

`swarm` é experimental e exige uma `SwarmBridge` autenticada composta pelo
host AppCore.

```text
node storage-only -> ArtifactStore(peer) ----+
node compute-only -> ComputeTarget(peer) ----+-> Auto planner -> execução
node combinado    -> ambos ------------------+
node local        -> CPU/GPU/NPU + cache ----+
```

- `Local` nunca consulta peer.
- `Swarm` exige rota remota autorizada e falha fechado.
- `Auto` compara custos permitidos; privacidade local sempre prevalece.

Anúncios expiram e expõem somente o budget após a contribution policy. Compute
remoto exige grant `ai.remote.compute`; storage remoto exige
`ai.remote.storage`. Artefatos grandes usam transfer separado do Peer RPC
genérico. Falhas de peer fazem failover limitado. A correção de um resultado
remoto geralmente não é verificável criptograficamente; essa limitação é
explícita.

## Segurança e observabilidade

`ModelSecurityPolicy` rejeita formatos provider/custom-op por default e limita
artefato/RAM/VRAM. `ProvenanceArtifactStore` delega assinatura à segurança do
AppCore. `Debug` redige prompts, conteúdo binário, outputs, embeddings, labels
e valores de metadata. Credenciais são somente referências.

`AiTelemetry` expõe p50/p95/p99 em buckets fixos, outcomes, admissions, loads,
fallback/escalation e placements sem labels de alta cardinalidade.
`AiObservationSink` é o ponto de integração com `appcore-ops`.

Snapshots por componente completam a visão operacional limitada:
`FairQueueMetrics` e `BatcherMetrics` expõem profundidade, saturação e itens;
`ResidencyMetrics` expõe reuso, loads pendentes, rollback, eviction e bytes;
`PeerArtifactMetrics` expõe bytes remotos verificados; `PeerDirectoryMetrics`
expõe disponibilidade, contribuição e churn agregados. Métricas de placement e
o observer de training completam a integração sem labels com IDs arbitrários.

`AiRuntime::model_loads()` expõe gauges ready/loading e contadores de hit,
waiter, loader, eviction e invalidation. Use-os para detectar cold loads
repetidos ou uma rota presa em loading; nenhum ID de modelo/backend é exposto.

## Níveis da API pública

Os exports planos são agrupados pela intenção de uso, não por promessa de
estabilidade:

| Nível | Tipos típicos | Consumidor |
|---|---|---|
| Essencial | `AiRuntime`, request/response/output, opções, limites, cancelamento e erros | aplicações que resolvem IA limitada |
| Política avançada | governor/admission, registries, scheduler, filas, batching, residency, artefatos, bundles, telemetria e segurança | composition root que ajusta placement e recursos |
| SPI de backend | `InferenceBackend`, descriptors/futures, `ArtifactStore`, peer transport, observations, planners, training e OpenAI transport opcionais | adapters de backend/provider/host |
| Interno | criação de rotas, permits de load, execution queue, scoring e codecs HTTP | implementação do crate; não exportado |

O grafo default não contém engine ML nem HTTP. `sha2` fornece identidade de
artefato; `libc` ou `windows-sys`, específicos do alvo, fornecem flags seguras
no-follow e contadores nativos de recursos. `nvml-wrapper` fica isolado atrás de
`accelerator-nvidia`; Candle e OpenAI-compatible continuam atrás de features
explícitas.
`#![deny(unsafe_code)]` vale para a crate; FFI nativa documentada e estritamente
limitada é permitida apenas nos módulos de recursos do macOS e Windows. A
descoberta Linux e o wrapper NVIDIA opcional usam APIs seguras.

## Evidência de performance e carga

`perf_lab` cobre resolve lightweight/miss/cold/warm, scaling 1/32/128 de
registry e scheduler, batch 1/2/4/8/16, leitura de artefato full/range, batch
Candle 1/8/32, training e Swarm 1/10/100/1.000. Gere JSONL e stress assim:

```bash
APPCORE_AI_BENCH_FORMAT=jsonl \
  cargo bench -p appcore-ai --bench perf_lab --all-features
APPCORE_AI_SOAK_ITERATIONS=100000 \
  cargo test -p appcore-ai --test stress_soak --all-features -- --nocapture
```

Veja o [relatório de otimização](benchmarks.pt.md) para antes/depois, memória,
custo intencional do hardening de artefatos e limites de interpretação.

## Padrões de uso

1. `resolve()` lightweight: passe um `AiRequest::text(TransformText, ..)` a um
   `AiRuntime` composto.
2. Modelo local forçado: use `execution = Local` e `options.model = Some(id)`.
3. Limites customizados: `AiResourceMode::Custom(AiResourceLimits { .. })`.
4. `Unrestricted`: use somente aceitando o warning de pressão/throttling.
5. Classificador opcional: execute `examples/candle_cpu.rs` com `backend-candle`.
6. LLM opcional: configure e execute `examples/openai_compatible.rs`.
7. Training: componha `TrainingJob`, `TrainingDataset`, `TrainingAdmission` e
   `CandleTrainer`.

## Limitações e gates

Não existe campo V1 de manifest nem flag CLI. A feature opt-in
`appcore-bin/ai-alpha` adiciona `ManifestApplicationHost::with_ai`, façade
`ApplicationAi`, handler `appcore.ai.resolve` e lifecycle graceful no Supervisor
sem alterar V1. Seleção declarativa ainda exige contrato pós-1.0 aceito.
Transporte, autenticação, replay store e isolamento pertencem ao
host/deployment; a crate não afirma sandbox nem zero trust.

```bash
cargo test -p appcore-ai --all-targets --all-features
./crates/appcore-ai/scripts/check-feature-matrix.sh
cargo bench -p appcore-ai --bench perf_lab --all-features
```
