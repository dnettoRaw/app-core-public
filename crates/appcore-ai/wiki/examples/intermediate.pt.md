# Inferência Candle local através do AiRuntime

[English](intermediate.en.md) | [Français](intermediate.fr.md) |
[Exemplo básico](basic.pt.md) | [Receitas](../recipes.pt.md) |
[Guia](../guide.pt.md)

Este exemplo percorre o fluxo completo: cria um artefato data-only, verifica e
armazena seus bytes, registra modelo e backend, aplica admission de recursos,
carrega o modelo sob demanda e executa classificação CPU via `AiRuntime`.

## Executar

```bash
cargo run -p appcore-ai --example candle_runtime --features backend-candle
```

Saída:

```text
class=class-a score=1.000
route=Local { backend: BackendId("candle/cpu-linear-v1"), device: DeviceId("local/cpu/candle") }
model_state=Ready
loads=1 local_placements=1 successes=1
```

O programa completo e compilado está em
[`examples/candle_runtime.rs`](../../examples/candle_runtime.rs). O exemplo
[`candle_cpu.rs`](../../examples/candle_cpu.rs) mostra a API SPI de backend
diretamente; em uma aplicação, prefira o fluxo com `AiRuntime` desta página.

## Dependência e feature

```toml
[dependencies]
appcore-ai = { version = "0.1.0-beta.3", default-features = false, features = ["backend-candle"] }
```

O build default continua sem Candle. A feature não baixa modelos e suporta
somente o formato limitado `NativeLinearV1` em CPU.

## 1. Criar identidade a partir dos bytes

O artefato possui 256 features determinísticas, duas classes, pesos e biases.
Ele é somente dados; não contém código ou custom op.

```rust
let dimensions = 256;
let mut weights = vec![0.0; dimensions * 2];
weights[usize::from(b'a')] = 10.0;
weights[dimensions + usize::from(b'b')] = 10.0;
let artifact = NativeLinearArtifact::new(
    dimensions,
    vec!["class-a".into(), "class-b".into()],
    weights,
    vec![0.0, 0.0],
)?;
let bytes = artifact.encode()?;
let identity = artifact.identity(None, false)?;
```

`identity` fixa SHA-256 e tamanho exato. Alterar um byte faz `store` ou `load`
retornar `AiError::Integrity`. Em produção, use `publisher` e
`signature_required = true` com `ProvenanceArtifactStore` quando a policy exigir
assinatura.

## 2. Guardar bytes e descrever o modelo

```rust
let memory = Arc::new(MemoryArtifactStore::new(4 * 1024 * 1024)?);
memory.store(&identity, &bytes, &CancellationToken::new())?;
let store: Arc<dyn ArtifactStore> = memory;

let descriptor = ModelDescriptor {
    id: ModelId::new("example/candle-runtime")?,
    revision: "v1".into(),
    tasks: vec![AiTask::ClassifyText],
    input_modalities: vec![AiModality::Text],
    format: ArtifactFormat::NativeLinearV1,
    quantization: Quantization::None,
    estimated_memory_bytes: u64::try_from(bytes.len())?.saturating_mul(2),
    estimated_vram_bytes: 0,
    max_input_bytes: 1_024,
    max_output_bytes: 1_024,
    context_limit: None,
    supported_backends: vec![BackendId::new(CANDLE_LINEAR_BACKEND_ID)?],
    supported_devices: vec![DeviceKind::Cpu],
    load_cost_units: 20,
    quality: Some(QualityTier::Tiny),
    artifact: identity,
};
```

Os tetos do descriptor são parte da admission. Não declare valores menores que
o pico real do backend.

## 3. Registrar sem descoberta implícita

```rust
let backends = Arc::new(BackendRegistry::new());
backends.register(Arc::new(CandleBackend::new(
    store,
    CandleBackendConfig::default(),
)?))?;

let models = Arc::new(ModelRegistry::new());
models.register(descriptor, [ArtifactLocation::Memory])?;
```

O estado inicial é `Available`: os bytes existem, mas o backend ainda não
carregou tensores. O primeiro `resolve` faz `Available -> Loading -> Ready`.
Registro duplicado falha; não existe substituição silenciosa.

## 4. Admission com capacidade explícita

`SystemHardwareProbe::default()` lê capacidade real de CPU/RAM no macOS, Linux
e Windows e mantém métricas de acelerador indisponíveis como `None`. O exemplo
usa `Custom` para impor um teto da aplicação menor que o host detectado:

```rust
request.options.resources = AiResourceMode::Custom(AiResourceLimits {
    max_cpu_percent: 80,
    max_memory_bytes: 16 * 1024 * 1024,
    max_vram_bytes: 0,
    max_workers: 1,
    max_concurrent_jobs: 1,
});
```

O backend estima 75% de CPU, um worker, zero VRAM e RAM igual ao maior entre o
descriptor e duas vezes o artefato. Se o teto for menor, o route é negado com
`AiError::Capacity("all model routes were denied")`.

## 5. Forçar modelo, privacidade e diagnóstico

```rust
let mut request = AiRequest::text(AiTask::ClassifyText, "a", limits)?;
request.options.execution = AiExecutionMode::Local;
request.options.privacy = AiPrivacyMode::LocalOnly;
request.options.model = Some(ModelId::new("example/candle-runtime")?);
request.options.resources = custom_limits;
request.options.include_diagnostics = true;

let response = runtime.resolve(request).await?;
```

Forçar o ID evita selecionar outro modelo compatível. `LocalOnly` impede tanto
compute quanto storage remoto. `include_diagnostics` expõe backend/device e
tentativas limitadas, sem copiar input, output ou credenciais.

## Estado e telemetria depois da chamada

```rust
assert_eq!(models.get(&model_id)?.state, ModelState::Ready);
let metrics = runtime.telemetry();
assert_eq!(metrics.requests, 1);
assert_eq!(metrics.model_load_successes, 1);
assert_eq!(metrics.local_placements, 1);
assert_eq!(metrics.successes, 1);
```

Uma segunda chamada reutiliza o modelo `Ready`; `model_load_successes` continua
em 1. Os percentis são buckets aproximados e as métricas não usam IDs de
modelo, tenant, peer ou prompt como labels.

## Falhas que o exemplo torna explícitas

| Situação | Resultado |
|---|---|
| feature `backend-candle` ausente | o backend nem faz parte da API compilada |
| digest ou tamanho divergente | `AiError::Integrity` |
| modelo sem localização | nenhuma rota local compatível |
| formato/task/device incompatível | rota excluída antes da inferência |
| RAM/CPU insuficiente | candidate negado pela admission |
| token cancelado antes do load | `AiError::Cancelled` |
| deadline consumida | `AiError::DeadlineExceeded` |
| backend duplicado | `AiError::Conflict("backend id")` |

Para checkpoints, resume e o uso do descriptor treinado, siga a receita de
[training local](../recipes.pt.md#training-candle-local-e-reprodutível).
