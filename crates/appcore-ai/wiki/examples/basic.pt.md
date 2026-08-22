# Runtime lightweight completo, sem framework de ML

[English](basic.en.md) | [Français](basic.fr.md) |
[Exemplo intermediário](intermediate.pt.md) | [Receitas](../recipes.pt.md) |
[Guia](../guide.pt.md)

Este exemplo monta um `AiRuntime` real, registra uma regra determinística,
executa uma classificação local e lê diagnóstico e telemetria. O build default
é suficiente: não há Candle, download de modelo ou acesso de rede.

## Executar

No workspace AppCore:

```bash
cargo run -p appcore-ai --example lightweight_runtime
```

Saída:

```text
label=operational score=1.000
route=Lightweight attempts=1
requests=1 successes=1 lightweight=1
```

O fonte compilado está em
[`examples/lightweight_runtime.rs`](../../examples/lightweight_runtime.rs).

## Dependência

Em um consumidor independente:

```toml
[dependencies]
appcore-ai = { version = "0.1.0-beta.1", default-features = false }
```

`appcore-ai` usa SemVer independente. Fixe a versão beta deliberadamente e
revise mudanças antes de atualizar.

## Composição mínima

```rust
use appcore_ai::{
    AiContributionPolicy, AiExecutionMode, AiLimits, AiPrivacyMode, AiRequest,
    AiResponse, AiResult, AiRuntime, AiTask, BackendRegistry, GovernorAdmission,
    LightweightEngine, ModelRegistry, ResourceGovernor, ResourceGovernorConfig,
    RuleMatch, SystemAiClock, SystemHardwareProbe, TextRule,
};
use std::sync::Arc;

fn build_runtime(limits: AiLimits) -> AiResult<AiRuntime> {
    let lightweight = LightweightEngine::new(
        vec![TextRule {
            label: "service.status".into(),
            pattern: "status".into(),
            output: "operational".into(),
            matching: RuleMatch::Exact,
        }],
        limits,
        8_000,
    )?;
    let governor = ResourceGovernor::new(
        SystemHardwareProbe::default(),
        ResourceGovernorConfig::default(),
        AiContributionPolicy::default(),
    )?;
    let admission = GovernorAdmission::new(governor, SystemAiClock::new());
    AiRuntime::new(
        limits,
        Arc::new(lightweight),
        Arc::new(ModelRegistry::new()),
        Arc::new(BackendRegistry::new()),
        Arc::new(admission),
    )
}

async fn classify(runtime: &AiRuntime, limits: AiLimits) -> AiResult<AiResponse> {
    let mut request = AiRequest::text(AiTask::ClassifyText, "status", limits)?;
    request.options.execution = AiExecutionMode::Local;
    request.options.privacy = AiPrivacyMode::LocalOnly;
    request.options.include_diagnostics = true;
    runtime.resolve(request).await
}
```

Mesmo sem modelos, `ModelRegistry`, `BackendRegistry` e `ModelAdmission` são
dependências obrigatórias. Isso mantém a composição explícita e permite trocar
o caminho lightweight por um backend sem mudar o contrato de request.

## O que cada limite protege

O exemplo executável reduz `max_input_bytes` e `max_output_bytes` para 256.
`AiLimits` também limita partes de input, metadata e tentativas. O mesmo valor
deve ser usado ao criar o input, construir o engine e compor o runtime.

Um input acima do teto falha antes de entrar no resolver:

```rust
let limits = AiLimits {
    max_input_bytes: 4,
    ..AiLimits::default()
};
let error = AiRequest::text(AiTask::TransformText, "cinco", limits)
    .expect_err("five bytes must exceed a four-byte limit");
assert!(matches!(error, appcore_ai::AiError::LimitExceeded { .. }));
```

## Outra operação sem modelo

`TransformText` normaliza whitespace e não consulta regras:

```rust
let mut request = AiRequest::text(
    AiTask::TransformText,
    "  um\t  texto\nlimitado  ",
    limits,
)?;
request.options.execution = AiExecutionMode::Local;
request.options.privacy = AiPrivacyMode::LocalOnly;
let response = runtime.resolve(request).await?;
assert_eq!(response.output, appcore_ai::AiOutput::Text("um texto limitado".into()));
```

## Garantias observáveis

- `LocalOnly` e `Local` impedem compute e storage remotos.
- `include_diagnostics` retorna rota e tentativas, nunca o prompt.
- `Debug` de request/response mostra tamanhos redigidos.
- sem regra compatível e sem modelo, o erro é
  `AiError::NotFound("compatible AI route")`.
- o exemplo não oferece recursos ao Swarm porque
  `AiContributionPolicy::default()` doa zero compute e zero storage.

Continue no [exemplo intermediário](intermediate.pt.md) para carregar um
artefato verificado e executar Candle através do mesmo `AiRuntime`.
