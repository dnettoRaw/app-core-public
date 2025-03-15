// =============================================================================
//        #######
//     ###       ###     F: shared_lease_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/02 00:04:12 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:07:11 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;

fn repository() -> FileLeaseRepository {
    let root = std::env::temp_dir().join(format!(
        "appcore-provider-lease-{}-{}",
        std::process::id(),
        unique()
    ));
    FileLeaseRepository::open(root, LeasePolicy::new(45, 10, 0).unwrap()).unwrap()
}

#[test]
fn acquire_heartbeat_release_cycle() {
    let repo = repository();
    let owner = LeaseOwner::new("owner-a").unwrap();
    let lease = repo.acquire("resource-a", owner, 100).unwrap();

    assert_eq!(lease.token.epoch, 1);
    assert_eq!(
        repo.check_fence(&lease.token, 110).unwrap(),
        LeaseDecision::Allowed
    );

    let renewed = repo
        .heartbeat(LeaseHeartbeat {
            token: lease.token.clone(),
            now_ms: 120,
        })
        .unwrap();
    assert_eq!(renewed.expires_at_ms, 165);

    repo.release(&lease.token).unwrap();
    assert_eq!(
        repo.check_fence(&lease.token, 130).unwrap(),
        LeaseDecision::NoLease
    );
}

#[test]
fn expired_lease_is_recovered_with_monotonic_fencing() {
    let repo = repository();
    let first = repo
        .acquire("resource-a", LeaseOwner::new("owner-a").unwrap(), 100)
        .unwrap();
    let second = repo
        .acquire("resource-a", LeaseOwner::new("owner-b").unwrap(), 200)
        .unwrap();

    assert_eq!(second.token.epoch, first.token.epoch + 1);
    assert_eq!(
        repo.check_fence(&first.token, 210).unwrap(),
        LeaseDecision::WrongOwner
    );
    assert_eq!(
        repo.check_fence(&second.token, 210).unwrap(),
        LeaseDecision::Allowed
    );
}

#[test]
fn concurrent_acquire_allows_one_owner() {
    let repo = Arc::new(repository());
    let mut handles = Vec::new();
    for index in 0..8 {
        let repo = Arc::clone(&repo);
        handles.push(thread::spawn(move || {
            repo.acquire(
                "resource-a",
                LeaseOwner::new(format!("owner-{index}")).unwrap(),
                100,
            )
            .is_ok()
        }));
    }
    let acquired = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .filter(|acquired| *acquired)
        .count();

    assert_eq!(acquired, 1);
}

#[test]
fn stale_epoch_is_rejected_after_same_owner_reacquires() {
    let repo = repository();
    let first = repo
        .acquire("resource-a", LeaseOwner::new("owner-a").unwrap(), 100)
        .unwrap();
    let second = repo
        .acquire("resource-a", LeaseOwner::new("owner-a").unwrap(), 200)
        .unwrap();

    assert_eq!(second.token.epoch, first.token.epoch + 1);
    assert_eq!(
        repo.check_fence(&first.token, 210).unwrap(),
        LeaseDecision::StaleEpoch
    );
}

#[test]
fn release_does_not_reset_the_fencing_epoch() {
    let repo = repository();
    let owner = LeaseOwner::new("owner-a").unwrap();
    let first = repo.acquire("resource-a", owner.clone(), 100).unwrap();
    repo.release(&first.token).unwrap();

    let second = repo.acquire("resource-a", owner, 200).unwrap();

    assert_eq!(second.token.epoch, first.token.epoch + 1);
    assert_eq!(
        repo.check_fence(&first.token, 210).unwrap(),
        LeaseDecision::StaleEpoch
    );
}

#[test]
fn fencing_high_water_mark_survives_repository_restart() {
    let repo = repository();
    let root = repo.root.clone();
    let policy = repo.policy();
    let first = repo
        .acquire("resource-a", LeaseOwner::new("owner-a").unwrap(), 100)
        .unwrap();
    repo.release(&first.token).unwrap();
    drop(repo);

    let reopened = FileLeaseRepository::open(root, policy).unwrap();
    let second = reopened
        .acquire("resource-a", LeaseOwner::new("owner-a").unwrap(), 200)
        .unwrap();

    assert_eq!(second.token.epoch, first.token.epoch + 1);
}

#[test]
fn reserved_epoch_after_interrupted_acquire_is_never_reused() {
    let repo = repository();
    let epoch_path = repo.epoch_path("resource-a").unwrap();
    write_epoch(&epoch_path, 7).unwrap();

    let lease = repo
        .acquire("resource-a", LeaseOwner::new("owner-a").unwrap(), 100)
        .unwrap();

    assert_eq!(lease.token.epoch, 8);
}

#[test]
fn exhausted_epoch_and_timestamp_overflow_fail_closed() {
    let repo = repository();
    let epoch_path = repo.epoch_path("resource-a").unwrap();
    write_epoch(&epoch_path, u64::MAX).unwrap();

    assert!(repo
        .acquire("resource-a", LeaseOwner::new("owner-a").unwrap(), 100)
        .is_err());
    assert!(repo
        .acquire("resource-b", LeaseOwner::new("owner-b").unwrap(), u64::MAX,)
        .is_err());
}

#[test]
fn heartbeat_rejects_a_regressing_clock() {
    let repo = repository();
    let lease = repo
        .acquire("resource-a", LeaseOwner::new("owner-a").unwrap(), 100)
        .unwrap();
    let renewed = repo
        .heartbeat(LeaseHeartbeat {
            token: lease.token.clone(),
            now_ms: 120,
        })
        .unwrap();

    assert!(repo
        .heartbeat(LeaseHeartbeat {
            token: renewed.token,
            now_ms: 119,
        })
        .is_err());
}

#[test]
fn policy_rejects_clock_skew_that_consumes_the_ttl() {
    assert!(LeasePolicy::new(45, 10, 45).is_err());
    assert!(LeasePolicy::new(45, 10, 46).is_err());
}

#[test]
fn malformed_unversioned_duplicate_and_oversized_states_fail_closed() {
    let repo = repository();
    let state_path = repo.state_path("resource-a").unwrap();
    let unversioned =
        "resource=resource-a\nowner=owner-a\nepoch=1\nacquired_at_ms=1\nheartbeat_at_ms=1\nexpires_at_ms=2\n";
    std::fs::write(&state_path, unversioned).unwrap();
    assert!(repo.current("resource-a").is_err());

    let duplicate = "format=appcore-shared-lease-v1\nresource=resource-a\nowner=owner-a\nepoch=1\nepoch=2\nacquired_at_ms=1\nheartbeat_at_ms=1\nexpires_at_ms=2\n";
    std::fs::write(&state_path, duplicate).unwrap();
    assert!(repo.current("resource-a").is_err());

    std::fs::write(&state_path, vec![b'x'; 4097]).unwrap();
    assert!(repo.current("resource-a").is_err());
}

#[test]
fn dotted_and_underscored_resources_do_not_collide() {
    let repo = repository();
    let dotted = repo
        .acquire("resource.a", LeaseOwner::new("owner-a").unwrap(), 100)
        .unwrap();
    let underscored = repo
        .acquire("resource_a", LeaseOwner::new("owner-b").unwrap(), 100)
        .unwrap();

    assert_eq!(dotted.token.resource, "resource.a");
    assert_eq!(underscored.token.resource, "resource_a");
    assert_eq!(
        repo.check_fence(&dotted.token, 110).unwrap(),
        LeaseDecision::Allowed
    );
    assert_eq!(
        repo.check_fence(&underscored.token, 110).unwrap(),
        LeaseDecision::Allowed
    );
}

fn unique() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}
