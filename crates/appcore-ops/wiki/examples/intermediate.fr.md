# Observations bornees et redigees

[English](intermediate.en.md) | [Português](intermediate.pt.md) |
[Exemple minimal](basic.fr.md) | [Guide](../guide.fr.md)

Enregistrez les faits operationnels dans un sink borne et conservez la
cardinalite dans des compteurs monotones. Les valeurs sensibles sont redigees
avant retention.

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

Le sink ne conserve que les deux evenements les plus recents. N'utilisez pas
les attributs pour des IDs non bornes ou des payloads bruts, meme avec redaction.
