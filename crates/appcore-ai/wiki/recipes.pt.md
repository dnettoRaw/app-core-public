# Receitas concretas do appcore-ai

[English](recipes.en.md) | [Français](recipes.fr.md) |
[Guia](guide.pt.md) | [Exemplo básico](examples/basic.pt.md) |
[Exemplo intermediário](examples/intermediate.pt.md)

Esta página parte de APIs reais do `0.1.0-beta.2`. Ela não presume campo V1 nem
backend oculto. Composição explícita existe em `appcore-bin/ai-alpha`; seleção
declarativa aguarda contrato AppCore pós-1.0.

## Escolha rápida

| Necessidade | Feature | Ponto de partida |
|---|---|---|
| normalizar/classificar por regra | nenhuma | [`lightweight_runtime.rs`](../examples/lightweight_runtime.rs) |
| inferência linear CPU via runtime | `backend-candle` | [`candle_runtime.rs`](../examples/candle_runtime.rs) |
| usar somente o SPI Candle | `backend-candle` | [`candle_cpu.rs`](../examples/candle_cpu.rs) |
| treinar e gravar checkpoints | `training-candle` | [`candle_training.rs`](../examples/candle_training.rs) |
| bridge para peers AppCore | `swarm` | `SwarmBridge` implementada pelo host |
| texto, tools ou visão generativa local/privada | `backend-openai-compatible` | [`openai_compatible.rs`](../examples/openai_compatible.rs) |

## Budgets locais e contribuição separados

`AiContributionPolicy` nunca amplia o budget local. Este exemplo mantém até
70% de CPU e 64 MiB para trabalho local, mas anuncia no máximo 25% de CPU,
8 MiB de RAM, dois workers e 512 MiB de artifact storage aos peers autorizados:

```rust
use appcore_ai::{
    AiContributionPolicy, AiResourceLimits, AiResourceMode, AiResult,
    ResourceGovernor, ResourceGovernorConfig, SystemHardwareProbe,
};

fn budgets() -> AiResult<()> {
    let contribution = AiContributionPolicy {
        contribute_compute: true,
        contribute_storage: true,
        max_cpu_percent: 25,
        max_gpu_percent: 0,
        max_memory_bytes: 8 * 1024 * 1024,
        max_vram_bytes: 0,
        max_storage_bytes: 512 * 1024 * 1024,
        max_workers: 2,
        max_concurrent_jobs: 1,
    };
    let governor = ResourceGovernor::new(
        SystemHardwareProbe::default(),
        ResourceGovernorConfig::default(),
        contribution,
    )?;
    let mode = AiResourceMode::Custom(AiResourceLimits {
        max_cpu_percent: 70,
        max_memory_bytes: 64 * 1024 * 1024,
        max_vram_bytes: 0,
        max_workers: 4,
        max_concurrent_jobs: 2,
    });
    let pair = governor.budgets(mode, 0)?;
    assert_eq!(pair.local.cpu_percent, 70);
    assert_eq!(pair.contribution.cpu_percent, 25);
    assert_eq!(pair.contribution.memory_bytes, Some(8 * 1024 * 1024));
    assert_eq!(pair.contribution.storage_bytes, 512 * 1024 * 1024);
    Ok(())
}
```

Para um node estritamente local, use `AiContributionPolicy::default()`. Os
budgets doados ficam em zero mesmo que o modo local seja `Performance` ou
`Unrestricted`.

## Cache local com SHA-256 e ativação atômica

`LocalArtifactCache` deriva o nome do digest; nomes externos nunca escolhem o
path final. O store valida digest e tamanho antes de criar o arquivo definitivo:

```rust
use appcore_ai::{
    ArtifactDigest, ArtifactIdentity, LocalArtifactCache,
};

let bytes = b"bounded-model-bytes";
let identity = ArtifactIdentity {
    digest: ArtifactDigest::from_bytes(bytes),
    size_bytes: u64::try_from(bytes.len())?,
    publisher: None,
    signature_required: false,
};
let root = std::env::temp_dir().join(format!(
    "appcore-ai-cache-example-{}",
    std::process::id()
));
let cache = LocalArtifactCache::new(&root, 1024)?;
let path = cache.store(&identity, bytes)?;
assert_eq!(cache.load(&identity)?, bytes);
assert_eq!(path, cache.path(identity.digest));
std::fs::remove_dir_all(root)?;
```

Se `bytes` mudar depois da criação da identidade, `store` falha com
`AiError::Integrity("artifact digest")`. Para assinatura obrigatória, envolva
um `ArtifactStore` com `ProvenanceArtifactStore`; o verifier é um adapter da
segurança AppCore, não uma chave privada dentro desta crate.

## Cancelamento cooperativo e deadline

O caller mantém o token e pode cancelar todas as suas cópias. O runtime verifica
cancelamento antes de route, load e inferência; backends também devem cooperar:

```rust
let cancellation = appcore_ai::CancellationToken::new();
let mut request = appcore_ai::AiRequest::text(
    appcore_ai::AiTask::TransformText,
    "bounded input",
    limits,
)?;
request.options.execution = appcore_ai::AiExecutionMode::Local;
request.options.deadline = Some(std::time::Duration::from_millis(250));

cancellation.cancel();
let result = runtime
    .resolve_with_cancellation(request, cancellation)
    .await;
assert_eq!(result, Err(appcore_ai::AiError::Cancelled));
```

Deadline é relativa ao início de `resolve`; ela não mata thread nem contorna um
backend bloqueante. O adapter de backend deve dividir trabalho longo e consultar
o token.

## Local, Auto e Swarm sem ambiguidade

| Modo | Compute remoto | Storage remoto | Bridge obrigatória |
|---|---:|---:|---:|
| `Local` | nunca | somente se explicitamente permitido e não `LocalOnly` | não |
| `Auto` | somente com grant e policy | somente com grant e policy | para candidatos remotos |
| `Swarm` | obrigatório | opcional e independente | sim |

Uma requisição que exige compute remoto precisa declarar policy e grant:

```rust
use appcore_ai::{
    AiAuthorizationContext, AiExecutionMode, AiPrivacyMode, CapabilityId,
    REMOTE_COMPUTE_GRANT,
};

request.options.execution = AiExecutionMode::Swarm;
request.options.privacy = AiPrivacyMode::TrustedSwarm;
request.options.distribution.allow_remote_compute = true;
request.options.distribution.allow_remote_storage = false;
request.options.authorization = Some(AiAuthorizationContext {
    tenant: CapabilityId::new("tenant/example")?,
    subject: CapabilityId::new("subject/example")?,
    grants: vec![CapabilityId::new(REMOTE_COMPUTE_GRANT)?],
});
```

Sem `runtime.with_swarm_bridge(...)`, o resultado é
`AiError::SwarmUnavailable`. Ativar storage remoto também exige
`REMOTE_STORAGE_GRANT`. `LocalOnly` combinado com qualquer permissão remota é
input inválido. A bridge deve reutilizar autenticação, discovery, replay e Peer
RPC existentes do AppCore.

## Training Candle local e reprodutível

Execute o job completo:

```bash
cargo run -p appcore-ai --example candle_training --features training-candle
```

Saída determinística do dataset do exemplo:

```text
checkpoint epoch=2 step=4 loss=0.6634
checkpoint epoch=4 step=8 loss=0.3914
epochs=4 steps=8 final_loss=0.3914 artifact_bytes=2090 stored=true
```

O programa configura dataset, seed, epochs, steps, batch, recursos e frequência
de checkpoint explicitamente. O `TrainingOutput` já contém bytes, identidade e
um `ModelDescriptor` pronto para registro:

```rust
let output = trainer
    .train(&job, dataset, progress, &cancellation)
    .await?;
models.register(
    output.descriptor.clone(),
    [appcore_ai::ArtifactLocation::Memory],
)?;
```

Use o mesmo `ArtifactStore` no `CandleTrainer` e no `CandleBackend`; o artefato
final já foi gravado pelo trainer. Para resume, atribua uma identidade verificada
a `job.resume`. Training distribuído não é suportado.

## Observações redigidas e métricas

Conecte `AiObservationSink` ao adapter `appcore-ops` da composição. Os eventos
não carregam prompt, output, model ID, peer ID ou credencial:

```rust
use appcore_ai::{AiObservation, AiObservationSink};

struct OpsAdapter;

impl AiObservationSink for OpsAdapter {
    fn record(&self, observation: &AiObservation) {
        match observation {
            AiObservation::RequestCompleted { success, attempts, .. } => {
                record_counter("ai.request.completed", *success, *attempts);
            }
            _ => record_event_class(observation),
        }
    }
}

let runtime = runtime.with_observation_sink(std::sync::Arc::new(OpsAdapter));
```

`record_counter` e `record_event_class` são funções do adapter do host, não APIs
desta crate. Para polling local, use `runtime.telemetry()` e publique apenas os
campos agregados.

## Backpressure antes do backend

Use uma `FairQueue` por domínio de dispatch e rejeite overload de forma
estruturada:

```rust
use appcore_ai::{
    AiPriority, CancellationToken, FairQueue, FairQueueConfig, QueueAdmission,
};
use std::time::Duration;

let mut queue = FairQueue::new(FairQueueConfig {
    capacity: 2,
    starvation_after: Duration::from_secs(1),
    overload_retry_after: Duration::from_millis(25),
})?;
assert!(matches!(
    queue.enqueue("one", AiPriority::Normal, 0, None, CancellationToken::new()),
    QueueAdmission::Queued { .. }
));
queue.enqueue("two", AiPriority::High, 0, None, CancellationToken::new());
let third = queue.enqueue(
    "three",
    AiPriority::Normal,
    0,
    None,
    CancellationToken::new(),
);
assert!(matches!(third, QueueAdmission::Rejected { .. }));
```

`DynamicBatcher` deve ser particionado por `BatchKey`: modelo, backend, device e
classe da task precisam ser idênticos. Não agrupe requests somente porque seus
inputs têm o mesmo tipo.

## Diagnóstico rápido

| Erro | Verifique primeiro |
|---|---|
| `NotFound("compatible AI route")` | task, model ID, state, localização, backend e device |
| `Capacity("all model routes were denied")` | `ResourceEstimate`, modo, RAM/VRAM conhecida e pressão |
| `Unauthorized` | tenant, grants separados de compute/storage e privacy |
| `SwarmUnavailable` | feature `swarm`, bridge composta e anúncios vivos |
| `Integrity` | digest, tamanho, publisher, assinatura e validade |
| `BackendUnavailable` | lifecycle do modelo e saúde do backend |
| `LimitExceeded` | limite indicado, tamanho real e attempts/peers/batch |

Erros são parte do contrato. Não implemente fallback silencioso, não habilite
`Unrestricted` automaticamente e não transforme capacidade desconhecida em
capacidade infinita.
