// =============================================================================
//        #######
//     ###       ###     F: metrics_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/06/04 11:57:41 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::{metric_retained_bytes, InMemoryMetrics, MAX_IN_MEMORY_METRICS, MAX_METRIC_NAME_BYTES};
use std::sync::Arc;
use std::thread;

#[test]
fn increments_and_snapshots_named_counter() {
    let metrics = InMemoryMetrics::new();
    assert_eq!(metrics.increment("runtime.tick"), 1);
    assert_eq!(metrics.increment("runtime.tick"), 2);
    assert_eq!(metrics.snapshot()[0].name, "runtime.tick");
    assert_eq!(metrics.snapshot()[0].value, 2);
}

#[test]
fn snapshot_is_stable_and_sorted() {
    let metrics = InMemoryMetrics::new();
    let _ = metrics.increment("z");
    let _ = metrics.increment("a");
    let snapshot = metrics.snapshot();
    assert_eq!(snapshot[0].name, "a");
    assert_eq!(snapshot[1].name, "z");
}

#[test]
fn concurrent_increments_are_not_lost() {
    let metrics = Arc::new(InMemoryMetrics::new());
    let handles = (0..4)
        .map(|_| {
            let metrics = Arc::clone(&metrics);
            thread::spawn(move || {
                for _ in 0..250 {
                    let _ = metrics.increment("runtime.tick");
                }
            })
        })
        .collect::<Vec<_>>();
    for handle in handles {
        handle.join().expect("metrics thread");
    }
    assert_eq!(metrics.snapshot()[0].value, 1_000);
}

#[test]
fn rejects_invalid_count_and_byte_pressure_without_losing_existing_counters() {
    let one_name = metric_retained_bytes("a");
    let metrics = InMemoryMetrics::with_limits(1, one_name);
    assert_eq!(metrics.try_increment("a"), Some(1));
    assert_eq!(metrics.try_increment("a"), Some(2));
    assert_eq!(metrics.try_increment("b"), None);
    assert_eq!(metrics.try_increment(""), None);
    assert_eq!(
        metrics.try_increment(&"x".repeat(MAX_METRIC_NAME_BYTES + 1)),
        None
    );

    let pressure = metrics.pressure();
    assert_eq!(pressure.entries, 1);
    assert_eq!(pressure.used_bytes, one_name);
    assert_eq!(pressure.peak_bytes, one_name);
    assert_eq!(pressure.count_rejections, 1);
    assert_eq!(pressure.name_rejections, 2);
    assert_eq!(metrics.snapshot()[0].value, 2);
}

#[test]
fn byte_budget_rejects_a_second_larger_name() {
    let one_name = metric_retained_bytes("a");
    let metrics = InMemoryMetrics::with_limits(2, one_name);
    assert_eq!(metrics.try_increment("a"), Some(1));
    assert_eq!(metrics.try_increment("longer"), None);
    assert_eq!(metrics.pressure().byte_rejections, 1);
}

#[test]
fn shared_snapshot_is_stable_and_reuses_names_across_updates() {
    let metrics = InMemoryMetrics::new();
    let _ = metrics.increment("runtime.first");
    let stable = metrics.shared_snapshot();
    let _ = metrics.increment("runtime.first");
    let _ = metrics.increment("runtime.second");

    assert_eq!(
        stable.iter().collect::<Vec<_>>(),
        vec![("runtime.first", 1)]
    );
    assert_eq!(
        metrics.shared_snapshot().iter().collect::<Vec<_>>(),
        vec![("runtime.first", 2), ("runtime.second", 1)]
    );
}

#[test]
fn constructor_clamps_untrusted_cardinality_without_preallocation() {
    let metrics = InMemoryMetrics::with_limits(usize::MAX, usize::MAX);
    let pressure = metrics.pressure();
    assert_eq!(pressure.max_entries, MAX_IN_MEMORY_METRICS);
    assert_eq!(pressure.entries, 0);
    assert!(metrics.shared_snapshot().is_empty());
}

#[test]
fn retained_generation_is_released_after_its_last_consumer() {
    let metrics = InMemoryMetrics::new();
    metrics.increment("counter");
    let snapshot = metrics.shared_snapshot();
    let other = snapshot.clone();
    let old_generation = Arc::downgrade(&snapshot.counters);
    let pressure = metrics.pressure();
    metrics.increment("counter");
    assert_eq!(snapshot.iter().next(), Some(("counter", 1)));
    assert_eq!(metrics.pressure(), pressure);
    drop(snapshot);
    assert!(old_generation.upgrade().is_some());
    drop(other);
    assert!(old_generation.upgrade().is_none());
    assert_eq!(
        metrics.shared_snapshot().iter().next(),
        Some(("counter", 2))
    );
}
