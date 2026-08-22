# Runtime adaptativo para LLMs e multimodal

[English](generative-llm.en.md) | [Français](generative-llm.fr.md) |
[Guia](guide.pt.md) | [Modelos e training](models.pt.md) |
[Arquitetura](architecture-adr.pt.md) | [Threat model](threat-model.pt.md)

> Status: `backend-openai-compatible` entrega transporte limitado de
> texto/chat/tools e visão opt-in para servidores configurados explicitamente.
> Candle continua sendo o backend classificador data-only. `AnalyzeDocument`
> ainda exige backend de documentos; não há parser PDF/OCR universal embutido.

O objetivo não é escolher um único engine nem incluir outro runtime como
dependência obrigatória. O `appcore-ai` será o plano de controle que escolhe a
melhor rota instalada para o pedido, hardware e política. Cada engine continua
isolado atrás de `InferenceBackend` e pode ser trocado sem alterar a aplicação.

Não há dependência do Colibri. O AppCore possui planejamento genérico e
verificável de VRAM/RAM/storage local, manifest de segmentos e leitor de ranges
verificados. Expert streaming só é anunciado por backend que consuma esses
segmentos de verdade.

As recomendações foram revisadas em 2026-08-21. Engines e modelos mudam
rapidamente: fixe versão, digest, formato, tokenizer, chat template e licença
em cada deployment.

## O que “faca suíça” significa

```text
AiRequest
  task       -> gerar texto | analisar imagem | analisar documento | decidir | embed
  input      -> Text | Image | Document | Audio | Video | Opaque
  quality    -> Fast | Balanced | Deep | Maximum
  latency    -> Interactive | Balanced | Throughput | Background
  placement  -> Local | Swarm | Auto
        |
        v
validação e privacy
  -> caminho determinístico quando suficiente
  -> modelos e adapters compatíveis com todas as modalidades
  -> admission por RAM/VRAM/CPU/deadline
  -> score por load, fila, latência, throughput, residency e custo
  -> engine persistente escolhido
  -> resposta limitada e diagnóstico redigido
```

“Suportar todos” significa aceitar adapters conformes, não compilar todos os
frameworks no core. O build default permanece pequeno. Uma instalação habilita
somente os adapters de que precisa.

## Velocidade versus profundidade

`AiOptions::quality` aplica um piso explícito; não é apenas uma sugestão:

| Perfil | Menor `QualityTier` admitido | Uso típico |
|---|---|---|
| `Fast` | `Tiny` | UI, autocomplete, classificação e respostas curtas |
| `Balanced` | `Small` | assistente local geral |
| `Deep` | `Balanced` | documentos, código e análise mais difícil |
| `Maximum` | `Large` | qualidade acima de latência e consumo |

```rust
let mut request = AiRequest::text(
    AiTask::GenerateText,
    "Compare as duas alternativas e justifique a conclusão.",
    AiLimits::default(),
)?;
request.options.quality = AiQualityTarget::Deep;
request.options.latency = AiLatencyClass::Balanced;
request.options.execution = AiExecutionMode::Auto;
request.options.allow_escalation = true;
request.options.deadline = Some(Duration::from_secs(45));

let answer = ai.resolve(request).await?;
```

Um model ID forçado vence o filtro automático de qualidade porque constitui uma
decisão explícita do caller. A rota ainda precisa satisfazer formato,
modalidades, device, privacy, recursos e deadline. Não há downgrade silencioso
de modelo ou quantização.

`Deep` não autoriza loops autônomos ilimitados nem exposição de chain of
thought. Um futuro planner multiestágio poderá executar draft, verificação e
synthesis somente com passos, tokens, custo e deadline limitados. A aplicação
continua dona do prompt, tools, schema e política de negócio.

## Imagem e PDF

A beta possui modalidades explícitas e valida semanticamente os pedidos:

```rust
let input = AiInput::new(
    vec![
        AiContent::Text("Liste os riscos visíveis nesta imagem".into()),
        AiContent::Binary {
            media_type: "image/png".into(),
            bytes: image_bytes,
        },
    ],
    limits,
)?;
let request = AiRequest {
    task: AiTask::AnalyzeImage,
    input,
    options: AiOptions::default(),
};
```

Para PDF:

```rust
let input = AiInput::new(
    vec![AiContent::Binary {
        media_type: "application/pdf".into(),
        bytes: pdf_bytes,
    }],
    limits,
)?;
let request = AiRequest {
    task: AiTask::AnalyzeDocument,
    input,
    options: AiOptions {
        quality: AiQualityTarget::Deep,
        ..AiOptions::default()
    },
};
```

PDF é um container, não uma modalidade nativa de todo VLM. Um adapter pode:

1. usar suporte documental nativo do engine;
2. extrair texto limitado e preservar número de página;
3. rasterizar somente páginas admitidas e enviá-las a um VLM;
4. aplicar OCR limitado quando não houver camada textual;
5. combinar resultados com referências de origem.

O core não embute parser PDF, OCR nem image decoder. Um processor deverá ter
limites de páginas, pixels, bytes expandidos, tempo e outputs. Nenhum PDF pode
causar expansão ilimitada ou download de recursos externos.

## Sistema decisório

`AiTask::Decide` não transforma uma completion em autoridade automática. A
ordem aconselhada é:

```text
regra determinística
  -> classificador pequeno quando a regra não basta
  -> LLM/VLM somente para ambiguidade permitida
  -> validação de schema e confidence
  -> política da aplicação aceita, rejeita ou pede revisão
```

Regras, outcomes e thresholds pertencem à aplicação. O Runtime fornece limites,
roteamento, idempotência da chamada, auditoria redigida e diagnóstico da rota.
Para ações importantes, exija output estruturado, evidência referenciada e um
fallback seguro; nunca use texto livre como comando privilegiado.

## Profiles de servidor suportados

A feature `backend-openai-compatible` possui profile explícito para todas as
famílias abaixo. O engine continua sendo processo implantado separadamente; a
feature não instala, baixa, inicia ou oferece sandbox. O deployment declara
modelos, devices, suporte a visão/tools/seed/stop, endpoint, timeout e limites.

| Hardware/workload | Adapter aconselhado | Motivo | Trade-off |
|---|---|---|---|
| CPU, GGUF, hardware variado | [llama.cpp](https://github.com/ggml-org/llama.cpp) | cobertura ampla, quantização e CPU/GPU híbrido | nem sempre é o mais rápido por device |
| Apple Silicon | [MLX-LM](https://github.com/ml-explore/mlx-lm) e adapter VLM MLX | memória unificada e kernels nativos | específico de Apple |
| NVIDIA doméstica, baixa concorrência | [ExLlamaV3](https://github.com/turboderp-org/exllamav3) via TabbyAPI | EXL3 e foco em GPUs consumer | formato/ecossistema especializado |
| NVIDIA/AMD, alta concorrência | [SGLang](https://www.sglang.io/) ou [vLLM](https://docs.vllm.ai/en/stable/) | batching contínuo, KV cache e serving multimodal | stack operacional maior |
| NVIDIA, desempenho máximo | [TensorRT-LLM](https://docs.nvidia.com/tensorrt-llm/) | kernels, quantização e serving NVIDIA | forte acoplamento ao hardware |
| Intel CPU/GPU/NPU | [OpenVINO GenAI](https://docs.openvino.ai/2026/openvino-workflow-generative/inference-with-genai.html) | pipelines LLM/VLM otimizados para Intel | formatos e conversão próprios |
| classificador pequeno Rust | Candle atual | in-process, data-only e auditável | não é LLM generativo |

O probe não escolhe por marca apenas. Ele mede o tuple completo:

```text
engine version + model revision + quantization + context + batch + device
```

E mantém pelo menos: cold start, TTFT, prompt tokens/s, decode tokens/s,
requests/s, RAM, VRAM, queue depth e taxa de erro. Uma configuração vence apenas
para a classe de workload em que foi medida.

## Adapter comum de servidor entregue

`OpenAiCompatibleBackend` traduz o contrato central uma vez para os servidores
listados. Ele fornece:

- endpoint loopback por default;
- binding exato de `ModelId` para nome do modelo no servidor;
- mensagens com papéis, sampling limitado, tools/tool calls e data URLs de imagem;
- erro explícito quando uma capability não foi declarada;
- limites de request/response, timeout e checagens de cancelamento;
- admissão justa e limitada ao redor das rotas de modelo;
- erros estáveis sem body do provider nem output privado do processo;
- trait de transporte que recebe somente referência de segredo AppCore.

O transporte HTTP default rejeita referência de credencial e serve apenas a
endpoints loopback/privados sem autenticação. Deployments remotos fornecem
transporte apoiado por AppCore security e usam o construtor explícito
`OpenAiCompatibleConfig::remote`. O processo do engine permanece carregado;
launch, health probe e sandbox do SO pertencem ao deployment.

```bash
APPCORE_AI_BASE_URL=http://127.0.0.1:8080 \
APPCORE_AI_MODEL=meu-modelo \
APPCORE_AI_MODEL_SHA256=<digest-hex-de-64-caracteres> \
cargo run -p appcore-ai --example openai_compatible \
  --features backend-openai-compatible
```

## Residência AppCore própria

A residency do modelo completo continua usando:

```text
ArtifactIdentity -> Vram(device) | Memory | LocalStorage | Peer(peer)
```

`ArtifactBundleManifest` e `SegmentedModelReader` agora implementam o boundary
de ranges independente de engine:

```text
ModelBundle
  dense/tokenizer/config  -> preferencialmente residente
  segment 000..N          -> digest + tamanho + offset + classe
  access observations     -> hot/warm/cold, agregadas e limitadas
  placement               -> VRAM -> RAM -> mmap/NVMe -> peer verificado
```

O leitor valida ranges ordenados e sem sobreposição, limites por segmento e por
request e SHA-256 de cada segmento carregado; `LocalArtifactCache::load_range`
não aloca o artifact completo. Invariantes adicionais:

- cada segmento tem identidade e digest antes da ativação;
- o core planeja bytes e tiers; o adapter continua dono de tensors e kernels;
- prefetch usa janela e concorrência limitadas;
- cache miss, eviction, rollback e I/O pressure são observáveis;
- parte densa pode ser pinada, mas nenhum peer força residency local;
- leitura remota sempre verifica bytes antes de cache/ativação;
- falha de storage ou pressão reduz a rota ou falha de forma controlada;
- não usar `LD_PRELOAD`, hook de filesystem ou formato oculto de terceiros.

Até esse bundle possuir backend consumidor, a documentação não afirma expert
streaming. Modelos maiores que a memória continuam inelegíveis pelo governor.

## Modelos iniciais

Escolha o modelo depois de escolher modalidade e engine. Valores de RAM/VRAM
dependem da quantização, contexto e cache e devem ser medidos no deployment.

| Modelo | Modalidades/uso | Perfil inicial |
|---|---|---|
| [Qwen3 4B](https://huggingface.co/Qwen/Qwen3-4B) | texto multilíngue local | `Fast`/`Balanced`, GGUF Q4/Q5 |
| [Qwen3 8B](https://huggingface.co/Qwen/Qwen3-8B) | resposta geral mais forte | `Balanced`/`Deep` |
| [Gemma 3 12B IT](https://huggingface.co/google/gemma-3-12b-it) | texto + imagem | VLM `Deep`; aceite os termos Gemma |
| [Phi-4 multimodal](https://huggingface.co/microsoft/Phi-4-multimodal-instruct) | texto, imagem e áudio | adapter SafeTensors/OpenVINO após conformance |
| [Mistral Small 3.2 24B](https://huggingface.co/mistralai/Mistral-Small-3.2-24B-Instruct-2506) | texto, visão, tools e documentos | `Deep`/`Maximum`, hardware maior |

Não habilite `trust_remote_code`. Um modelo que exige custom code só entra por
adapter explicitamente revisado e por policy própria. Consulte
[modelos e training](models.pt.md) para o suporte realmente entregue.

## Entregue e gates externos

Entregue na beta: modalidades/qualidade, chat com papéis, sampling/tools
limitados, adapter comum, sete profiles de engine, teste real em loopback,
admissão justa, manifests/ranges segmentados e composição opt-in do Supervisor
e capabilities no `appcore-bin`.

Não afirmado: streaming de tokens, accounting de KV cache do engine, instalação
automática/sandbox do processo, PDF/OCR, expert streaming, adapter Swarm de Peer
RPC em produção ou manifest V2 declarativo. Esses itens são gates explícitos,
não promessas da documentação. O core não promete qualquer modelo em qualquer
máquina; ele explica por que a rota foi admitida ou recusada.
