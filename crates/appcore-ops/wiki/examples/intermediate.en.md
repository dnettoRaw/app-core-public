# Bounded and redacted observations

[Português](intermediate.pt.md) | [Français](intermediate.fr.md) |
[Minimal example](basic.en.md) | [Guide](../guide.en.md)

Record operational facts in a bounded sink and keep cardinality in monotonic
counters. Sensitive attribute values are redacted before retention.

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

The sink retains only the newest two events. Do not use observation attributes
for unbounded IDs or raw application payloads even though values are redacted.
