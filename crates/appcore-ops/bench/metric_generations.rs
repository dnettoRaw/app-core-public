// =============================================================================
//        #######
//     ###       ###     F: metric_generations.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: working-tree by dnettoRaw
//    ##   ## ##   ##    U: working-tree by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Interleaved snapshots and updates with bounded consumer generation retention.

use appcore_ops::MetricSnapshot;
use std::collections::VecDeque;
use std::hint::black_box;

pub(super) const CASES: [&str; 3] = [
    "metric_update_4096_retained_0",
    "metric_update_4096_retained_1",
    "metric_update_4096_retained_16",
];

pub(super) fn run(selected: Option<&str>) {
    for (case, retained) in CASES.into_iter().zip([0, 1, 16]) {
        if selected.is_some_and(|value| value != case) {
            continue;
        }
        let metrics = super::metric_fixture();
        let mut generations = VecDeque::<(MetricSnapshot, u64)>::with_capacity(retained);
        let mut value = 1;
        // Warm the exact retention policy outside timing so even one timed
        // iteration exercises steady-state retention and eviction.
        for _ in 0..retained {
            generations.push_back((metrics.shared_snapshot(), value));
            value = metrics.increment("runtime.metric.0000");
        }
        super::measure(case, 1_000, || {
            if retained == 0 {
                drop(black_box(metrics.shared_snapshot()));
            } else {
                // Evict before admitting the next generation; never retain N+1.
                drop(generations.pop_front());
                generations.push_back((metrics.shared_snapshot(), value));
            }
            value = black_box(metrics.increment(black_box("runtime.metric.0000")));
        });
        assert_eq!(generations.len(), retained);
        for (snapshot, expected) in &generations {
            assert_eq!(snapshot.len(), 4_096);
            assert_eq!(
                snapshot.iter().next(),
                Some(("runtime.metric.0000", *expected))
            );
        }
        assert_eq!(metrics.pressure().entries, 4_096);
    }
}
