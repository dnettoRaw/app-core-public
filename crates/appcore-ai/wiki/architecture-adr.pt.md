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

## Evidência comparativa

A pesquisa usa documentação e artigos disponíveis na data do ADR. Desempenho
relatado pelos projetos não é resultado AppCore; cada otimização exige benchmark
próprio reproduzível.

| Projeto | Técnica | Benefício | Custo ou restrição | Decisão AppCore | Fase |
|---|---|---|---|---|---|
| Lumabri | Doação independente de storage/compute, busca sob demanda, mirror local esparso e failover de réplicas | Peers só com CPU ou storage contribuem; bytes frequentes ficam locais | Rede/segurança experimental; primeiro acesso depende da rede | Separar `ArtifactStore` e `ComputeTarget`, anúncios expirantes e cache local verificado; sem hooks de filesystem | beta experimental |
| llama.cpp | GGUF, quantização ampla, offload híbrido CPU/GPU, mmap e batching contínuo no servidor | Portabilidade local e operação com VRAM insuficiente | Build C/C++, servidor em evolução rápida e fronteira de crash nativo | Profile OpenAI-compatible entregue; lifecycle do processo no deployment | adapter beta |
| vLLM | PagedAttention, batching contínuo, cache de prefixo/KV e serving distribuído | Throughput alto e menor fragmentação KV em geração concorrente | Stack Python/GPU pesada; técnicas dependem da workload | Batching por chave de compatibilidade e accounting limitado de cache, não sua API/runtime | otimização posterior |
| SGLang | Reuso de prefixo RadixAttention, batching contínuo, prefill em chunks e separação prefill/decode | Prefixos repetidos e workloads mistas eficientes | Stack de serving pesada e scheduling complexo de aceleradores | Extensões de prefix-cache e estágios separados privadas até demanda medida | otimização posterior |
| Burn | Modelos/training Rust com backends de tensor plugáveis e import ONNX | Training opcional coerente e portabilidade | Custo de build/dependências; aplicação define semântica do modelo | Avaliado, não selecionado; evitar segundo framework sem necessidade medida | somente pesquisa |
| Candle | Runtime de tensors Rust; CPU, CUDA, Metal e WASM; carga safetensors/ggml | Integração Rust e inferência local portátil | Integração de modelo/tokenizer continua específica | Primeiro backend CPU e trainer opt-in; tipos Candle fora dos contratos centrais | adapter beta |
| ONNX Runtime | Descoberta de capabilities dos Execution Providers, particionamento de grafo e arenas | Execução madura entre CPU, GPU e várias NPUs | Distribuição nativa e compatibilidade de providers; API tensor não é API texto | Seleção de device por capability; candidato a backend tensor limitado | backend futuro |
| TensorRT-LLM | Batching inflight, cache KV paginado, quantização, multi-GPU/multi-node | Throughput NVIDIA e otimizações maduras de serving | Especialização NVIDIA/Linux e footprint operacional grande | Profile de servidor compatível entregue; otimizações do engine fora do core | adapter beta, engine externo |

O fluxo público será limitado e observável:

```text
validar request
  -> classificar e validar modalidades de entrada
  -> tentar resolvers leves determinísticos
  -> encontrar modelos compatíveis
  -> aplicar piso de qualidade Fast/Balanced/Deep/Maximum
  -> calcular budgets local e de contribuição
  -> planejar localização dos artifacts
  -> planejar localização do compute
  -> admitir em fila/batch limitado
  -> executar por backend
  -> escalation opcional com teto de tentativas
  -> retornar trace redigido quando solicitado
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

`Auto` pode comparar ou combinar recursos locais e remotos, mas não autoriza
transferência silenciosa de dados: privacidade e distribuição continuam
governadas pelas policies. Bytes de peers são limitados e verificados antes
da ativação. A entrada pública `AiRuntime::resolve` é assíncrona e observável;
escalation tem tentativas limitadas e o trace redigido é opt-in.

- contratos: requests/responses/options validados, IDs, policies, limites,
  cancelamento e diagnóstico seguro;
- resolver leve: transformações, regras, matching e extração bounded;
- router: ordenação de candidatos, escalation com limite fixo e enforcement de policies;
- governor: snapshots de probes, hysteresis e budgets local/contribuição separados;
- scheduler: admission e score determinístico local/remoto;
- registry: metadata, lifecycle, capabilities e identidade/localização de artifacts;
- backend SPI: load/unload/inference/health e training especializado;
- batching: filas compatíveis limitadas, deadlines, cancelamento e falhas parciais;
- residency: promoção, prefetch e eviction limitados por tier de storage suportado;
- bridge distribuída: visões autenticadas e expirantes de peers, invocação de
  compute e fronteiras de transferência de artifacts;
- composition root: providers, capabilities, Supervisor e deployment policy.

## Decisões do alpha

- O core e os testes determinísticos funcionam sem GPU, rede ou download.
- Capacidade desconhecida nunca significa capacidade ilimitada.
- `Unrestricted` remove somente a margem voluntária AppCore, não proteções
  do SO, driver, firmware, temperatura ou alimentação elétrica.
- Filas, retries, peers, metadata, inputs, outputs, artifacts e transfers
  possuem limites explícitos.
- Peer remoto não é backend: backend define como executar, target onde e
  store onde os bytes residem.
- Candle `0.11` é o único framework de ML selecionado, somente por meio de
  `backend-candle` e `training-candle`.
- O primeiro formato é o classificador data-only e limitado `NativeLinearV1`;
  tipos Candle não atravessam a API central.
- integração explícita de deployment entrega composição no Supervisor e
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
- lifecycle/capability opt-in real no deployment.

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

## Consequências

A decisão acrescenta fronteiras de orquestração antes dos engines caros e exige
configuração mais explícita. Alguns modos retornam erro tipado de
indisponibilidade. Em troca, o core default permanece portátil e testável,
mudanças de backend ficam atrás do SPI e futuras integrações podem usar um novo
contrato versionado sem enfraquecer V1. O custo opcional de Candle permanece
explícito, não é transferido ao build mínimo.

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
