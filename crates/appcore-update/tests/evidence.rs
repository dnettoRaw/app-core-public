// =============================================================================
//        #######
//     ###       ###     F: evidence.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/24 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/24 00:00:00 by dnettoRaw
//      ###########      S: 1.0.3-rc
// =============================================================================
// appcore-norm: test

use appcore_contracts::{ApplicationId, BuildId};
use appcore_update::{ArtifactDescriptor, QuarantineReason, QuarantineStore, UpdateError};
use std::collections::BTreeSet;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

struct CheckpointHarness {
    seen: BTreeSet<&'static str>,
}

impl CheckpointHarness {
    fn mark(&mut self, checkpoint: &'static str) {
        assert!(
            self.seen.insert(checkpoint),
            "duplicate checkpoint: {checkpoint}"
        );
    }
}

#[test]
fn quarantine_process_lock_preserves_bounded_concurrent_writes() {
    let root = std::env::temp_dir().join(format!(
        "appcore-update-evidence-concurrency-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let store = Arc::new(QuarantineStore::open(&root, 4).unwrap());
    let barrier = Arc::new(Barrier::new(4));
    let handles = (0..4)
        .map(|index| {
            let store = Arc::clone(&store);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                let descriptor = ArtifactDescriptor::new(
                    ApplicationId::new("evidence.application").unwrap(),
                    format!("1.0.{index}"),
                    BuildId::new(format!("evidence-build-{index}")).unwrap(),
                    "stable",
                    ">=1.0.0",
                    "1",
                    format!("provider://evidence/{index}"),
                    format!("{:064x}", index + 1),
                    1,
                )
                .unwrap();
                for _attempt in 0..100 {
                    match store.quarantine(
                        &descriptor,
                        QuarantineReason::ManualReview("fixture evidence".to_string()),
                        index as u64,
                    ) {
                        Ok(_) => return,
                        Err(UpdateError::Recovery(message))
                            if message.contains("lock is unavailable") =>
                        {
                            thread::sleep(Duration::from_millis(2));
                        }
                        Err(error) => panic!("unexpected quarantine failure: {error:?}"),
                    }
                }
                panic!("quarantine lock did not become available");
            })
        })
        .collect::<Vec<_>>();
    for handle in handles {
        handle.join().unwrap();
    }
    let mut checkpoints = CheckpointHarness {
        seen: BTreeSet::new(),
    };
    checkpoints.mark("concurrent_writes_complete");
    checkpoints.mark("bounded_entries_persisted");
    assert_eq!(store.list().unwrap().len(), 4);
    assert_eq!(checkpoints.seen.len(), 2);
    std::fs::remove_dir_all(root).unwrap();
}
