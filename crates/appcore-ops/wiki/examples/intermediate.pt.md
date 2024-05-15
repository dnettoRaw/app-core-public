# Observacoes limitadas e redigidas

[English](intermediate.en.md) | [Français](intermediate.fr.md) |
[Exemplo minimo](basic.pt.md) | [Guia](../guide.pt.md)

Registre fatos operacionais em um sink limitado e mantenha cardinalidade em
contadores monotonicos. Valores sensiveis sao redigidos antes da retencao.

```rust
use appcore_ops::{
    InMemoryMetrics, InMemoryObservationSink, ObservationEvent, ObservationKind,
    ObservationSeverity, ObservationSink,
};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let token = std::env::var("APPCORE_EXAMPLE_TOKEN")?;
    let metrics = InMemoryMetrics::new();
    let observations = InMemoryObservationSink::new(2);

    observations.emit(
        ObservationEvent::new(
            ObservationKind::ControlPlane,
            ObservationSeverity::Info,
            "control_plane.heartbeat.accepted",
            1_700_000_000_000,
        )
        .with_trace_id("trace-42")
        .with_attribute("region", "eu-west")
        .with_attribute("authorization", token),
    );
    metrics.increment("control_plane.heartbeat.accepted");

    observations.emit(ObservationEvent::new(
        ObservationKind::ControlPlane,
        ObservationSeverity::Warning,
        "control_plane.degraded",
        1_700_000_001_000,
    ));
    observations.emit(ObservationEvent::new(
        ObservationKind::ControlPlane,
        ObservationSeverity::Info,
        "control_plane.recovered",
        1_700_000_002_000,
    ));

    let retained = observations.snapshot();
    println!("retained={} counters={}", retained.len(), metrics.snapshot().len());
    Ok(())
}
```

O sink mantem somente os dois eventos mais novos. Nao use atributos de
observacao para IDs sem limite ou payloads brutos, mesmo com redacao.
