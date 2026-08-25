# ADR 0001: arquitetura de orquestração do AppCore AI

- Estado: aceito para implementação em `0.1.0-beta.1`
- Data: 2026-08-21
- Escopo: `appcore-ai`; sem alteração de manifest ou protocolo AppCore V1

[Perfil de LLMs generativos](generative-llm.pt.md) |
[Modelos e training](models.pt.md)

## Contexto e decisão

`appcore-ai` será uma crate da camada Runtime com SemVer independente. O build
default continuará leve e útil sem LLM. Frameworks de aceleração e training
serão opt-in e seus tipos não aparecerão nos contratos centrais.

A pesquisa comparou as fontes primárias de
[Lumabri](https://github.com/JustVugg/lumabri),
[llama.cpp](https://github.com/ggml-org/llama.cpp),
[vLLM](https://docs.vllm.ai/),
[SGLang](https://github.com/sgl-project/sglang),
[Burn](https://burn.dev/books/burn/),
[Candle](https://huggingface.github.io/candle/),
[ONNX Runtime](https://onnxruntime.ai/docs/reference/high-level-design.html) e
[TensorRT-LLM](https://nvidia.github.io/TensorRT-LLM/).

| Evidência | Benefício absorvido | Limite preservado |
|---|---|---|
| Lumabri | doação separada de storage/compute, mirror local e failover | sem hooks de filesystem ou protocolo paralelo |
| llama.cpp | portabilidade, quantização e CPU/GPU híbrido | profile OpenAI-compatible opt-in entregue; processo pertence ao deployment |
| vLLM/SGLang | batching compatível e contabilidade de KV/prefix cache | somente após benchmark próprio |
| Burn | workflow Rust de training/inference | avaliado, mas não selecionado para evitar um segundo framework |
| Candle | integração Rust para training/inference | selecionado somente nas features opt-in beta |
| ONNX Runtime | seleção por capability de execution provider | tensor não vira API central de texto |
| TensorRT-LLM | batching/KV cache de alto throughput | integração NVIDIA apenas futura |

O fluxo público será limitado e observável:

```text
validar modalidade -> caminho leve -> piso de qualidade -> modelos -> budget -> artifact placement
        -> compute placement -> admission -> execução -> escalation limitada
```

Desde o primeiro alpha:

```rust
pub enum AiExecutionMode {
    Local,
    Swarm,
    Auto,
}
```

Compute e storage são decisões independentes. `InferenceBackend` descreve
como executar, `ComputeTarget` onde executar e `ArtifactStore` onde os bytes
estão. A identidade do artifact é derivada do conteúdo e não muda quando sua
localização muda.

## Ownership

- contratos: requests, responses, IDs, policies, limites e diagnóstico seguro;
- resolver leve: transformações, regras, matching e extração bounded;
- router: candidatos e escalation com limite fixo;
- governor: budget local e budget de contribuição separados;
- scheduler: admission e score determinístico local/remoto;
- registry: metadata, lifecycle e localizações de artifacts;
- backend SPI: load/unload/inference/health e training especializado;
- batching/residency: filas, promoção, prefetch e eviction limitados;
- bridge distribuída: peers autenticados e anúncios com expiração;
- composition root: providers, capabilities, Supervisor e deployment policy.

## Decisões do alpha

- Candle `0.11` é o único framework de ML selecionado, somente por meio de
  `backend-candle` e `training-candle`.
- O primeiro formato é o classificador data-only e limitado `NativeLinearV1`;
  tipos Candle não atravessam a API central.
- `appcore-bin/ai-alpha` entrega composição explícita no Supervisor e
  `CapabilityRegistry` sem alterar V1; seleção declarativa aguarda contrato
  pós-1.0 versionado.

## Decisão de detecção de recursos

Descoberta de hardware é uma pequena fronteira de plataforma atrás de
`HardwareProbe`, não outro framework/provider. Topologia estática é cacheada
separadamente de contadores dinâmicos, e `HardwareSampler` é on-demand,
single-flight e limitado. Valores desconhecidos continuam desconhecidos;
falhas viram categorias estáveis e redigidas.

CPU/RAM usam interfaces nativas limitadas do SO. GPU Apple integrada usa
memória unificada. DRM sysfs oferece dados AMD e fallback NVIDIA best-effort.
NVML fica na feature opcional `accelerator-nvidia`, pois APIs portáteis do SO
não expõem memória framebuffer e utilização NVIDIA exatas. Só queries read-only
são usadas; não existe controle de clock, potência ou ventoinha.

Admission usa device exato. VRAM dedicada nunca é somada entre GPUs; memória
unificada é cobrada uma vez do pool de RAM. Modos calculam headroom voluntário
da disponibilidade atual e hysteresis reduz oscilações. Batching, treino,
residency e contribuição Swarm consomem a mesma visão limitada.

Isso exige FFI nativa pequena e documentada no macOS e Windows. A crate usa
`#![deny(unsafe_code)]`, com `allow` apenas nesses módulos de plataforma. O
restante permanece safe Rust. Veja [recursos de hardware](resources.pt.md).

## Implementação generativa beta e limites restantes

A beta entrega:

- adapter OpenAI-compatible limitado e sete profiles explícitos de servidor;
- chat com papéis, sampling, tools/tool calls, usage e imagem opt-in;
- engine externo persistente, loopback default e nenhum download na inferência;
- manifest de segmentos AppCore e ranges locais verificados;
- load single-flight por modelo/backend em fallback e concorrência;
- lifecycle/capability opt-in real no `appcore-bin`.

Streaming nativo exige transporte de deployment explicitamente capaz. Ficam
fora da claim: PDF/OCR, launch ou sandbox automático, accounting de KV cache do engine, expert streaming sem backend consumidor e
manifests V2 declarativos.

Essa fronteira mantém crash nativo, tokenizer, KV cache e kernels fora do core
backend-neutral. O [perfil generativo](generative-llm.pt.md) contém modelos,
budgets, comandos e gates.

## Fora de `0.1.0`

- criar internamente outro framework de deep learning ou tensor;
- downloads silenciosos, filas/transfers ilimitados ou custom ops inseguros;
- distributed training, consenso, NAT traversal ou outro control plane;
- extensão silenciosa dos contratos V1;
- afirmar que `Unrestricted` desliga proteções físicas;
- promover RC/stable sem as evidências exigidas.

O Swarm só fica operacional quando uma bridge autenticada for instalada. Os
testes com peers simulados comprovam o planner, não uma rede de produção. O
runtime pode verificar identidade de artifacts e autenticar peers, mas não
promete prova criptográfica geral da correção de um resultado remoto. Ativar
Candle aumenta materialmente a árvore opcional de dependências; o build default
continua sem framework de ML.

## Emenda beta.2 de 2026-08-25

O SPI OpenAI-compatible agora retorna futures boxed para permitir HTTP nativo
assíncrono sem escolher executor para o core. O cliente default limitado isola
o transporte standalone bloqueante atrás de um máximo de threads curtas. Ele
não bloqueia o executor chamador e rejeita excesso em vez de criar fila sem
limite.

Streaming usa `AiStreamSink` síncrono: retornar de um evento autoriza ler o
próximo chunk. Isso torna backpressure explícito sem channel específico de
runtime. Cancelamento é checado entre chunks, output parcial nunca vira resposta
completa e conteúdo bruto não entra em diagnósticos internos. Extensões de
provider são JSON limitado com campos centrais reservados; fallback de JSON
Schema é sempre escolhido pelo chamador. Nenhum manifest ou wire contract V1 é
alterado.
