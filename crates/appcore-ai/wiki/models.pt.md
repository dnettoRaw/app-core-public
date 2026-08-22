# Modelos, configuração e training

[English](models.en.md) | [Français](models.fr.md) |
[Guia](guide.pt.md) | [LLMs generativos](generative-llm.pt.md) |
[Exemplo Candle](examples/intermediate.pt.md) | [Receitas](recipes.pt.md)

Esta página separa formatos reconhecidos, backends realmente executáveis e
training implementado. Em `0.1.0-beta.1`, registrar metadata de um formato não
significa que exista um engine capaz de inferir esse formato.

## Matriz real de suporte

| Formato | Policy default | Backend incluído | Training incluído |
|---|---:|---:|---:|
| `NativeLinearV1` | aceita | Candle CPU | classificação linear local |
| GGUF | aceita | servidor llama.cpp/generic OpenAI-compatible | nenhum |
| ONNX | aceita | servidor OpenVINO/generic OpenAI-compatible | nenhum |
| SafeTensors | aceita | servidor MLX/vLLM/SGLang/TensorRT/Tabby/generic OpenAI-compatible | nenhum |
| `Other(CapabilityId)` | negado por default | adapter obrigatório | adapter obrigatório |

A crate entrega `candle/cpu-linear-v1` e `OpenAiCompatibleBackend` opt-in. O
segundo conversa com servidor já iniciado; não interpreta GGUF/ONNX/SafeTensors
e nunca baixa modelo silenciosamente. Binding exato, digest, device, formato e
capabilities continuam sendo configuração obrigatória do deployment.

`ModelDescriptor::input_modalities` e `BackendDescriptor::input_modalities`
declaram a interseção real aceita pela rota. O router rejeita, por exemplo, uma
imagem para um adapter somente texto mesmo quando o model ID foi registrado por
engano. `AiOptions::quality` também filtra automaticamente o `QualityTier`
mínimo; um model ID forçado não contorna modalidade, formato, recursos ou
privacy.

## Configurar um modelo generativo existente

Ative `backend-openai-compatible`, inicie o engine separadamente em loopback e
associe o `ModelId` ao nome exato no servidor. O exemplo exige o digest real:

```bash
APPCORE_AI_ENGINE=llama.cpp \
APPCORE_AI_FORMAT=gguf \
APPCORE_AI_BASE_URL=http://127.0.0.1:8080 \
APPCORE_AI_MODEL=meu-modelo \
APPCORE_AI_MODEL_SHA256=<digest-hex-de-64-caracteres> \
APPCORE_AI_MODEL_BYTES=<tamanho-exato> \
cargo run -p appcore-ai --example openai_compatible \
  --features backend-openai-compatible
```

Profiles: `llama.cpp`, `mlx-lm`, `vllm`, `sglang`, `tensorrt-llm`, `openvino`,
`tabbyapi` e `generic`. Tools, visão, seed e stop só devem ser habilitados no
config após prova no tuple servidor/modelo. O transporte default rejeita
credenciais; autenticação remota exige transporte apoiado por AppCore security.

## Configurar um NativeLinearV1 existente

Ative somente inferência:

```toml
[dependencies]
appcore-ai = { version = "0.1.0-beta.1", default-features = false, features = ["backend-candle"] }
```

Construa ou importe a matriz `[classes, input_dimensions]`, biases e labels:

```rust
let dimensions = 256;
let labels = vec!["available".into(), "unavailable".into()];
let weights = vec![0.0_f32; labels.len() * dimensions];
let biases = vec![0.0_f32; labels.len()];
let artifact = NativeLinearArtifact::new(
    dimensions,
    labels,
    weights,
    biases,
)?;
let bytes = artifact.encode()?;
let identity = artifact.identity(None, false)?;
```

Depois:

1. grave os bytes em um `ArtifactStore`;
2. crie um `ModelDescriptor` com `ArtifactFormat::NativeLinearV1`;
3. registre `CandleBackend` no `BackendRegistry`;
4. registre o descriptor e localização no `ModelRegistry`;
5. envie `AiTask::ClassifyText` pelo `AiRuntime`.

O fluxo completo está em
[`candle_runtime.rs`](../examples/candle_runtime.rs). Pesos são `f32`; declare
`Quantization::None`. Os outros valores de `Quantization` existem para backends
futuros e não quantizam automaticamente este formato.

## Treinar classificação local

Ative:

```toml
[dependencies]
appcore-ai = { version = "0.1.0-beta.1", default-features = false, features = ["training-candle"] }
```

Cada exemplo contém texto não vazio e o índice da classe:

```rust
let dataset: Arc<dyn TrainingDataset> = Arc::new(
    InMemoryTrainingDataset::new(
        vec![
            TrainingExample { text: "service ready".into(), label: 0 },
            TrainingExample { text: "healthy".into(), label: 0 },
            TrainingExample { text: "service failed".into(), label: 1 },
            TrainingExample { text: "unavailable".into(), label: 1 },
        ],
        1_000,
        512,
    )?,
);
```

Configure todos os limites do job:

```rust
let job = TrainingJob {
    id: CapabilityId::new("job/service-status")?,
    model: ModelId::new("model/service-status")?,
    revision: "v1".into(),
    labels: vec!["available".into(), "unavailable".into()],
    input_dimensions: 256,
    epochs: 20,
    max_steps: 1_000,
    batch_size: 16,
    learning_rate: 0.1,
    seed: 42,
    resource_requirements: ResourceEstimate {
        cpu_percent: 60,
        memory_bytes: 32 * 1024 * 1024,
        workers: 1,
        ..ResourceEstimate::default()
    },
    resource_mode: AiResourceMode::Custom(AiResourceLimits {
        max_cpu_percent: 70,
        max_memory_bytes: 64 * 1024 * 1024,
        max_vram_bytes: 0,
        max_workers: 1,
        max_concurrent_jobs: 1,
    }),
    checkpoints: TrainingCheckpointPolicy {
        every_epochs: 5,
        max_checkpoints: 4,
    },
    resume: None,
    publisher: None,
    max_input_bytes: 512,
    max_output_bytes: 4 * 1024,
};
```

Execute e registre o resultado:

```rust
let output = trainer
    .train(&job, dataset, progress, &cancellation)
    .await?;
models.register(output.descriptor.clone(), [ArtifactLocation::Memory])?;
```

Use o mesmo `ArtifactStore` no trainer e backend: o artefato final e os
checkpoints já são gravados pelo `CandleTrainer`. O programa reproduzível está
em [`candle_training.rs`](../examples/candle_training.rs):

```bash
cargo run -p appcore-ai --example candle_training --features training-candle
```

## Significado dos parâmetros

| Campo | Efeito |
|---|---|
| `labels` | ordem estável das classes; o dataset usa índices nessa ordem |
| `input_dimensions` | largura do vetor de features hash; não é número de tokens |
| `epochs` | máximo de passagens completas pelo dataset |
| `max_steps` | teto global que pode interromper antes do último epoch |
| `batch_size` | pedido do job; modos conservadores podem reduzi-lo |
| `learning_rate` | taxa SGD finita e positiva |
| `seed` | inicialização reproduzível dos pesos |
| `resource_requirements` | pico declarado antes da admission |
| `checkpoints` | frequência e quantidade máximas de snapshots |
| `resume` | identidade exata de um `NativeLinearV1` compatível |

`Eco` usa batch efetivo 1. `Balanced` e `Custom` reduzem o batch pedido pela
metade, arredondando para cima. `Performance` e `Unrestricted` preservam o
pedido, ainda sujeito ao teto do trainer.

## Limites default efetivos

| Limite do Candle trainer | Default |
|---|---:|
| exemplos | 100.000 |
| dimensões | 4.096 |
| classes | 256, com mínimo de 2 para training |
| epochs | 100 |
| optimizer steps | 100.000 |
| batch | 512 |
| artefato codificado | 64 MiB |

Labels têm no máximo 96 bytes. Cada texto também precisa respeitar o limite do
dataset e `job.max_input_bytes`. O limite efetivo sempre é o menor entre policy,
job, backend, artifact store e request.

## Escolher dimensões e dataset

`NativeLinearV1` usa features determinísticas derivadas dos bytes do texto. É
adequado para sinais lexicais simples, routing, filtros e classificações
pequenas. Como ponto inicial, não como garantia de qualidade:

| Problema | Dimensões iniciais |
|---|---:|
| até 20 labels, vocabulário pequeno | 256–512 |
| 20–100 labels ou vocabulário maior | 1.024–2.048 |
| até 256 labels | 2.048–4.096, medindo colisões e RAM |

Separe treino/validação, mantenha exemplos balanceados e meça precision,
recall e matriz de confusão. Mais epochs não corrigem labels ruins, classes
ambíguas ou falta de exemplos representativos.

## Resume, identidade e provenance

Resume exige as mesmas dimensões e labels, na mesma ordem. Nesta beta, o
trainer aceita identidade somente local com `signature_required = false`; a integração
de resume assinado ainda não está ligada ao trainer. Não altere bytes mantendo
o mesmo ID: SHA-256 e tamanho formam a identidade real.

Para ativação assinada em inferência, use `ProvenanceArtifactStore` com um
verifier fornecido pela segurança AppCore. `ModelSecurityPolicy::default()`
permite NativeLinearV1, GGUF, ONNX e SafeTensors, bloqueia formatos provider e
impõe tetos de artifact/RAM/VRAM. Uma deployment deve reduzir esses máximos ao
hardware real.

## O que não é training de LLM

O trainer atual não faz pretraining, fine-tuning, LoRA, geração, embeddings,
imagem, áudio, GPU nem training distribuído. LLMs devem ser treinados ou
fine-tuned fora do Runtime, convertidos para um formato data-only e ativados por
um backend explícito. Veja o [perfil de LLMs generativos](generative-llm.pt.md).
