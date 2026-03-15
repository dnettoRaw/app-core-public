// =============================================================================
//        #######
//     ###       ###     F: observability.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Runtime-owned observation hub and local production drain bootstrap.

use super::*;

pub(super) fn start_observations(
    config: &RuntimeConfig,
) -> Result<
    (
        InMemoryObservationSink,
        Arc<InMemoryMetrics>,
        FileObservationSink,
    ),
    BootstrapError,
> {
    let observations = InMemoryObservationSink::default();
    let metrics = Arc::new(InMemoryMetrics::new());
    let path = std::path::Path::new(&config.storage_path).join("runtime-observations.jsonl");
    let file_sink =
        FileObservationSink::new(FileObservationSinkConfig::new(path)).map_err(|error| {
            BootstrapError::Runtime(format!("failed to initialize observation drain: {error}"))
        })?;
    observations.add_drain(Arc::new(file_sink.clone()));
    observations.add_drain(Arc::new(ObservationMetricsSink::new(Arc::clone(&metrics))));
    observations.emit(
        ObservationEvent::new(
            ObservationKind::Lifecycle,
            ObservationSeverity::Info,
            "runtime.bootstrap.started",
            now_ms(),
        )
        .with_attribute("runtime_mode", format!("{:?}", config.runtime_mode)),
    );
    Ok((observations, metrics, file_sink))
}
