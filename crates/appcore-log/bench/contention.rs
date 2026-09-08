// =============================================================================
//        #######
//     ###       ###     F: contention.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: working-tree by dnettoRaw
//    ##   ## ##   ##    U: working-tree by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Four-producer batches measure synchronous contention, including rendezvous.

use appcore_log::{
    FileSink, FileSinkConfig, LogDispatcher, LogError, LogEvent, LogPolicy, LogSink,
    LOG_SIZE_64_MIB,
};
use std::sync::{Arc, Barrier, Mutex};
use std::time::Duration;

pub(super) const CASES: [&str; 3] = [
    "jsonl_concurrent_buffered_batch_64",
    "jsonl_concurrent_synced_batch_64",
    "serialized_slow_sink_batch_4",
];

pub(super) fn run(selected: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    for (index, case) in CASES.into_iter().enumerate() {
        if selected.is_some_and(|value| value != case) {
            continue;
        }
        if index == 2 {
            let sink = Arc::new(SlowSink(Mutex::new(0)));
            concurrent(case, sink.clone(), 1);
            assert_eq!(*sink.0.lock().expect("counter"), super::iterations(100) * 4);
        } else {
            let directory = super::benchmark_directory(case)?;
            let sink = Arc::new(
                FileSink::new(FileSinkConfig {
                    path: directory.join("application.jsonl"),
                    max_bytes: LOG_SIZE_64_MIB,
                    sync_each_write: index == 1,
                    retention: 1,
                    archive: None,
                })
                .expect("valid file sink"),
            );
            concurrent(case, sink, 16);
            std::fs::remove_dir_all(directory)?;
        }
    }
    Ok(())
}

fn concurrent(case: &str, sink: Arc<dyn LogSink>, events_per_worker: usize) {
    let dispatcher = LogDispatcher::new(LogPolicy::default(), vec![sink]);
    let rounds = super::iterations(100);
    // The owner is the fifth participant. Workers exist before timing starts;
    // two rendezvous per batch are included, thread creation/join are not.
    let barrier = Barrier::new(5);
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let dispatcher = &dispatcher;
            let barrier = &barrier;
            scope.spawn(move || {
                barrier.wait();
                for _ in 0..rounds {
                    barrier.wait();
                    for _ in 0..events_per_worker {
                        dispatcher.event(0, "application").info("concurrent event");
                    }
                    barrier.wait();
                }
            });
        }
        barrier.wait();
        super::measure(case, 100, || {
            barrier.wait();
            barrier.wait();
        });
    });
    assert_eq!(dispatcher.stats(), appcore_log::LogStats::default());
}

struct SlowSink(Mutex<u64>);

impl LogSink for SlowSink {
    fn emit(&self, _event: &LogEvent) -> Result<(), LogError> {
        let mut count = self.0.lock().expect("slow sink lock");
        // Models serialized blocking only: this is not a physical-disk result.
        std::thread::sleep(Duration::from_millis(1));
        *count += 1;
        Ok(())
    }
}
