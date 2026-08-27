// =============================================================================
//        #######
//     ###       ###     F: state_file_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/27 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/27 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0
// =============================================================================

use crate::{
    DurableTaskMisfirePolicyV1, FileSchedulerStateProvider, SchedulerStateClaimRequestV1,
    SchedulerStateCompletionV1, SchedulerStateError, SchedulerStateProvider,
    SchedulerStateRegistrationV1, SchedulerStateStatsV1,
};
use std::fs;
use std::sync::{Arc, Barrier};

fn registration() -> SchedulerStateRegistrationV1 {
    SchedulerStateRegistrationV1 {
        task_id: "task-a".to_string(),
        definition_hash: "a".repeat(64),
        initial_next_run_ms: 100,
        misfire_policy: DurableTaskMisfirePolicyV1::FireOnce,
    }
}

fn claim_request(owner_id: &str, now_ms: u64) -> SchedulerStateClaimRequestV1 {
    SchedulerStateClaimRequestV1 {
        task_id: "task-a".to_string(),
        owner_id: owner_id.to_string(),
        now_ms,
        claim_ttl_ms: 10,
        max_clock_skew_ms: 5,
    }
}

#[test]
fn terminal_receipt_survives_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("scheduler.json");
    let provider = FileSchedulerStateProvider::new(&path).unwrap();
    provider.register(&registration(), 4).unwrap();
    let claim = provider
        .try_claim(&claim_request("owner-a", 100))
        .unwrap()
        .unwrap();
    provider
        .complete(&SchedulerStateCompletionV1 {
            claim,
            completed_at_ms: 105,
            next_run_ms: None,
            settled: true,
        })
        .unwrap();

    let reopened = FileSchedulerStateProvider::new(path).unwrap();
    assert_eq!(
        reopened.stats().unwrap(),
        SchedulerStateStatsV1 {
            records: 1,
            claimed: 0,
            completed: 1,
        }
    );
    assert_eq!(
        reopened.try_claim(&claim_request("owner-b", 1_000)),
        Ok(None)
    );
}

#[test]
fn expired_claim_takeover_is_atomic_and_fences_old_owner() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("scheduler.json");
    let first_provider = FileSchedulerStateProvider::new(&path).unwrap();
    first_provider.register(&registration(), 4).unwrap();
    let first = first_provider
        .try_claim(&claim_request("owner-a", 100))
        .unwrap()
        .unwrap();

    let second_provider = FileSchedulerStateProvider::new(&path).unwrap();
    let second = second_provider
        .try_claim(&claim_request("owner-b", 116))
        .unwrap()
        .unwrap();
    assert_eq!(second.fencing_epoch, 2);
    assert_eq!(
        first_provider.complete(&SchedulerStateCompletionV1 {
            claim: first,
            completed_at_ms: 106,
            next_run_ms: None,
            settled: true,
        }),
        Err(SchedulerStateError::Fenced)
    );
}

#[test]
fn concurrent_providers_admit_one_claim() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("scheduler.json");
    FileSchedulerStateProvider::new(&path)
        .unwrap()
        .register(&registration(), 4)
        .unwrap();
    let barrier = Arc::new(Barrier::new(8));
    let handles = (0..8)
        .map(|index| {
            let path = path.clone();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                let provider = FileSchedulerStateProvider::new(path).unwrap();
                barrier.wait();
                provider
                    .try_claim(&claim_request(&format!("owner-{index}"), 100))
                    .unwrap()
                    .is_some()
            })
        })
        .collect::<Vec<_>>();
    let admitted = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .filter(|admitted| *admitted)
        .count();
    assert_eq!(admitted, 1);
}

#[test]
fn changed_definition_is_rejected_after_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("scheduler.json");
    FileSchedulerStateProvider::new(&path)
        .unwrap()
        .register(&registration(), 4)
        .unwrap();
    let mut changed = registration();
    changed.definition_hash = "b".repeat(64);
    assert!(matches!(
        FileSchedulerStateProvider::new(path)
            .unwrap()
            .register(&changed, 4),
        Err(SchedulerStateError::Conflict(_))
    ));
}

#[test]
fn checksum_corruption_fails_closed() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("scheduler.json");
    FileSchedulerStateProvider::new(&path)
        .unwrap()
        .register(&registration(), 4)
        .unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    value["checksum"] = serde_json::Value::String("0".repeat(64));
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(matches!(
        FileSchedulerStateProvider::new(path),
        Err(SchedulerStateError::InvalidState("invalid state checksum"))
    ));
}

#[test]
fn unknown_format_hits_update_wall() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("scheduler.json");
    FileSchedulerStateProvider::new(&path)
        .unwrap()
        .register(&registration(), 4)
        .unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    value["format"] = serde_json::Value::String("appcore-scheduler-state-v2".to_string());
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(matches!(
        FileSchedulerStateProvider::new(path),
        Err(SchedulerStateError::UpdateRequired)
    ));
}

#[test]
fn unknown_fields_and_oversized_files_fail_closed() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("scheduler.json");
    FileSchedulerStateProvider::new(&path)
        .unwrap()
        .register(&registration(), 4)
        .unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    value["removed_field"] = serde_json::Value::Bool(true);
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(matches!(
        FileSchedulerStateProvider::new(&path),
        Err(SchedulerStateError::InvalidState("invalid state file"))
    ));

    fs::File::create(&path)
        .unwrap()
        .set_len(4 * 1024 * 1024 + 1)
        .unwrap();
    assert!(matches!(
        FileSchedulerStateProvider::new(path),
        Err(SchedulerStateError::InvalidState("invalid state file"))
    ));
}

#[cfg(unix)]
#[test]
fn symlink_state_and_parent_paths_are_rejected() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let real = directory.path().join("real");
    fs::create_dir(&real).unwrap();
    let linked_parent = directory.path().join("linked");
    symlink(&real, &linked_parent).unwrap();
    assert!(matches!(
        FileSchedulerStateProvider::new(linked_parent.join("scheduler.json")),
        Err(SchedulerStateError::Unavailable)
    ));

    let target = real.join("target.json");
    fs::write(&target, b"{}").unwrap();
    let linked_state = real.join("state.json");
    symlink(target, &linked_state).unwrap();
    assert!(matches!(
        FileSchedulerStateProvider::new(linked_state),
        Err(SchedulerStateError::Unavailable)
    ));
}
