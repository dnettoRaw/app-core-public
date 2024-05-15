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

use super::InMemoryMetrics;
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
