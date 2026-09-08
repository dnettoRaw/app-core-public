// =============================================================================
//        #######
//     ###       ###     F: observation_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/02 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/02 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Tests bounded in-memory observation retention and shared snapshots.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Weak;

#[test]
fn bounded_sink_discards_oldest_event() {
    let sink = InMemoryObservationSink::new(2);
    for index in 0..3 {
        sink.emit(ObservationEvent::new(
            ObservationKind::Lifecycle,
            ObservationSeverity::Info,
            format!("runtime.event.{index}"),
            index,
        ));
    }
    let snapshot = sink.snapshot();
    assert_eq!(snapshot.len(), 2);
    assert_eq!(snapshot[0].name, "runtime.event.1");
    assert_eq!(sink.pressure().entries, 2);
    assert_eq!(sink.pressure().evictions, 1);
}

#[test]
fn sink_redacts_sensitive_attributes() {
    let sink = InMemoryObservationSink::new(2);
    sink.emit(
        ObservationEvent::new(
            ObservationKind::Security,
            ObservationSeverity::Warning,
            "security.rejected",
            1,
        )
        .with_attribute("access_token", "raw-secret"),
    );
    assert_eq!(sink.snapshot()[0].attributes["access_token"], "[REDACTED]");
}

#[test]
fn sensitive_attribute_matching_is_ascii_case_insensitive() {
    assert!(is_sensitive_key("X-PRIVATE_KEY-ID"));
    assert!(is_sensitive_key("Access-ToKeN"));
    assert!(is_sensitive_key("tokenized-count"));
    assert!(!is_sensitive_key("queue-count"));
    assert!(!is_sensitive_key("tókén"));
}

#[test]
fn sink_bounds_names_values_and_attribute_count() {
    let mut event = ObservationEvent::new(
        ObservationKind::Diagnostic,
        ObservationSeverity::Info,
        "n".repeat(1_000),
        1,
    );
    for index in 0..100 {
        event = event.with_attribute(format!("key-{index}"), "v".repeat(2_000));
    }
    let sink = InMemoryObservationSink::new(1);
    sink.emit(event);
    let event = &sink.snapshot()[0];

    assert!(event.name.len() <= MAX_OBSERVATION_NAME_BYTES);
    assert_eq!(event.attributes.len(), MAX_OBSERVATION_ATTRIBUTES);
    assert!(event
        .attributes
        .values()
        .all(|value| value.len() <= MAX_OBSERVATION_VALUE_BYTES));
}

#[test]
fn sink_revalidates_public_attribute_fields_before_retention() {
    let mut event = ObservationEvent::new(
        ObservationKind::Diagnostic,
        ObservationSeverity::Info,
        "runtime.direct",
        1,
    );
    for index in 0..100 {
        event.attributes.insert(
            format!("{}-token={index}", "k".repeat(1_000)),
            "private".repeat(1_000),
        );
    }
    let sink = InMemoryObservationSink::new(1);

    sink.emit(event);

    let event = &sink.snapshot()[0];
    assert!(event.attributes.len() <= MAX_OBSERVATION_ATTRIBUTES);
    assert!(event
        .attributes
        .keys()
        .all(|key| key.len() <= MAX_OBSERVATION_KEY_BYTES));
    assert!(event.attributes.values().all(|value| value == "[REDACTED]"));
}

#[test]
fn byte_pressure_evicts_oldest_and_shared_snapshot_stays_stable() {
    let first = ObservationEvent::new(
        ObservationKind::Diagnostic,
        ObservationSeverity::Info,
        "first",
        1,
    )
    .with_attribute("detail", "x".repeat(256))
    .redacted();
    let one_event = observation_retained_bytes(&first);
    let sink = InMemoryObservationSink::with_max_bytes(10, one_event);
    sink.emit(first);
    let stable = sink.shared_snapshot();

    sink.emit(
        ObservationEvent::new(
            ObservationKind::Diagnostic,
            ObservationSeverity::Info,
            "other",
            2,
        )
        .with_attribute("detail", "x".repeat(256)),
    );

    assert_eq!(stable.len(), 1);
    assert_eq!(stable.iter().next().unwrap().name, "first");
    assert_eq!(sink.shared_snapshot().iter().next().unwrap().name, "other");
    assert_eq!(
        sink.pressure(),
        InMemoryObservationPressure {
            entries: 1,
            max_entries: 10,
            used_bytes: one_event,
            peak_bytes: one_event,
            max_bytes: one_event,
            evictions: 1,
            oversized_rejections: 0,
            drain_rejections: 0,
        }
    );
}

#[test]
fn oversized_event_is_forwarded_but_not_retained() {
    let drain = Arc::new(InMemoryObservationSink::new(1));
    let sink = InMemoryObservationSink::with_max_bytes(2, 1);
    sink.add_drain(drain.clone());

    sink.emit(ObservationEvent::new(
        ObservationKind::Health,
        ObservationSeverity::Warning,
        "health.large",
        1,
    ));

    assert!(sink.is_empty());
    assert_eq!(sink.pressure().oversized_rejections, 1);
    assert_eq!(drain.len(), 1);
}

#[test]
fn constructor_clamps_count_without_preallocating_unbounded_memory() {
    let sink = InMemoryObservationSink::new(usize::MAX);
    let pressure = sink.pressure();
    assert_eq!(pressure.max_entries, MAX_IN_MEMORY_OBSERVATION_ITEMS);
    assert_eq!(pressure.max_bytes, MAX_IN_MEMORY_OBSERVATION_BYTES);
    assert!(sink.is_empty());
}

#[test]
fn drain_attachments_are_bounded_and_rejections_are_visible() {
    let sink = InMemoryObservationSink::new(1);
    for _ in 0..MAX_OBSERVATION_DRAINS {
        assert!(sink.try_add_drain(Arc::new(InMemoryObservationSink::new(1))));
    }
    assert!(!sink.try_add_drain(Arc::new(InMemoryObservationSink::new(1))));
    assert_eq!(sink.drain_count(), MAX_OBSERVATION_DRAINS);
    assert_eq!(sink.pressure().drain_rejections, 1);
}

#[test]
fn drain_configuration_is_copy_on_write() {
    let sink = InMemoryObservationSink::new(1);
    assert!(sink.try_add_drain(Arc::new(InMemoryObservationSink::new(1))));
    let first_generation = Arc::clone(&sink.drains.read());

    assert!(sink.try_add_drain(Arc::new(InMemoryObservationSink::new(1))));
    let second_generation = Arc::clone(&sink.drains.read());

    assert_eq!(first_generation.len(), 1);
    assert_eq!(second_generation.len(), 2);
    assert!(!Arc::ptr_eq(&first_generation, &second_generation));
}

struct ReentrantDrain {
    sink: Weak<InMemoryObservationSink>,
    calls: Arc<AtomicUsize>,
}

impl ObservationSink for ReentrantDrain {
    fn emit(&self, _event: ObservationEvent) {
        let sink = self.sink.upgrade().unwrap();
        self.calls.fetch_add(1, Ordering::Relaxed);
        assert!(sink.try_add_drain(Arc::new(InMemoryObservationSink::new(1))));
    }
}

#[test]
fn drain_callbacks_run_outside_the_configuration_lock() {
    let sink = Arc::new(InMemoryObservationSink::new(1));
    let calls = Arc::new(AtomicUsize::new(0));
    assert!(sink.try_add_drain(Arc::new(ReentrantDrain {
        sink: Arc::downgrade(&sink),
        calls: Arc::clone(&calls),
    })));

    sink.emit(ObservationEvent::new(
        ObservationKind::Diagnostic,
        ObservationSeverity::Info,
        "runtime.reentrant",
        1,
    ));

    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(sink.drain_count(), 2);
}

#[test]
fn in_memory_fanout_retains_one_shared_event_allocation() {
    let sink = InMemoryObservationSink::new(1);
    let drains = (0..MAX_OBSERVATION_DRAINS)
        .map(|_| Arc::new(InMemoryObservationSink::new(1)))
        .collect::<Vec<_>>();
    for drain in &drains {
        assert!(sink.try_add_drain(drain.clone()));
    }

    sink.emit(
        ObservationEvent::new(
            ObservationKind::Diagnostic,
            ObservationSeverity::Info,
            "runtime.shared",
            1,
        )
        .with_attribute("detail", "x".repeat(MAX_OBSERVATION_VALUE_BYTES)),
    );

    let retained = Arc::clone(sink.state.lock().events.front().unwrap());
    for drain in drains {
        let forwarded = Arc::clone(drain.state.lock().events.front().unwrap());
        assert!(Arc::ptr_eq(&retained, &forwarded));
    }
}

#[derive(Default)]
struct LegacyOwnedDrain {
    events: Mutex<Vec<ObservationEvent>>,
}

impl ObservationSink for LegacyOwnedDrain {
    fn emit(&self, event: ObservationEvent) {
        self.events.lock().push(event);
    }
}

#[test]
fn shared_dispatch_preserves_owned_sink_compatibility_and_redaction() {
    let mut event = ObservationEvent::new(
        ObservationKind::Security,
        ObservationSeverity::Warning,
        "security.shared",
        1,
    );
    event
        .attributes
        .insert("access_token".to_string(), "private".to_string());
    let event = SharedObservationEvent::new(event);
    let sink = LegacyOwnedDrain::default();

    sink.emit_shared(&event);

    assert_eq!(
        sink.events.lock()[0].attributes["access_token"],
        "[REDACTED]"
    );
}
