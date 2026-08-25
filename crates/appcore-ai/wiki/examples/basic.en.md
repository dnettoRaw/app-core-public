# Complete lightweight runtime without an ML framework

[Português](basic.pt.md) | [Français](basic.fr.md) |
[Intermediate example](intermediate.en.md) | [Recipes](../recipes.en.md) |
[Guide](../guide.en.md)

This example composes a real `AiRuntime`, registers a deterministic rule,
runs a local classification, and reads diagnostics and telemetry. The default
build is enough: there is no Candle dependency, model download, or network
access.

## Run it

From the AppCore workspace:

```bash
cargo run -p appcore-ai --example lightweight_runtime
```

Output:

```text
label=operational score=1.000
route=Lightweight attempts=1
requests=1 successes=1 lightweight=1
```

The compiled source is
[`examples/lightweight_runtime.rs`](../../examples/lightweight_runtime.rs).

## Dependency

In an independent consumer:

```toml
[dependencies]
appcore-ai = { version = "0.1.0-beta.2", default-features = false }
```

`appcore-ai` uses independent SemVer. Pin the beta version deliberately and
review changes before upgrading.

## Minimal composition

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

Even with no model, `ModelRegistry`, `BackendRegistry`, and `ModelAdmission`
are mandatory dependencies. This keeps composition explicit and lets the host
add a backend without changing the request contract.

## What each bound protects

The executable example lowers `max_input_bytes` and `max_output_bytes` to 256.
`AiLimits` also bounds input parts, metadata, and attempts. Use the same value
when creating input, building the engine, and composing the runtime.

Input above the ceiling fails before entering the resolver:

```rust
let limits = AiLimits {
    max_input_bytes: 4,
    ..AiLimits::default()
};
let error = AiRequest::text(AiTask::TransformText, "five!", limits)
    .expect_err("input must exceed the four-byte limit");
assert!(matches!(error, appcore_ai::AiError::LimitExceeded { .. }));
```

## Another model-free operation

`TransformText` normalizes whitespace without consulting rules:

```rust
let mut request = AiRequest::text(
    AiTask::TransformText,
    "  bounded\t  text\ninput  ",
    limits,
)?;
request.options.execution = AiExecutionMode::Local;
request.options.privacy = AiPrivacyMode::LocalOnly;
let response = runtime.resolve(request).await?;
assert_eq!(response.output, appcore_ai::AiOutput::Text("bounded text input".into()));
```

## Observable guarantees

- `LocalOnly` and `Local` exclude remote compute and storage.
- `include_diagnostics` returns routes and attempts, never the prompt.
- request and response `Debug` output reports redacted sizes.
- with no compatible rule or model, the error is
  `AiError::NotFound("compatible AI route")`.
- the example contributes no Swarm resources because
  `AiContributionPolicy::default()` donates zero compute and zero storage.

Continue with the [intermediate example](intermediate.en.md) to load a verified
artifact and execute Candle through the same `AiRuntime`.
