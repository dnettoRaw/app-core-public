// =============================================================================
//        #######
//     ###       ###     F: runtime_durable_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/27 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/27 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0
// =============================================================================
// appcore-norm: test

use crate::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

fn config() -> SchedulerConfig {
    SchedulerConfig {
        max_tasks: 16,
        max_concurrent_tasks: 2,
        poll_interval: Duration::from_millis(2),
    }
}

fn durable_config(owner_id: &str) -> DurableSchedulerConfigV1 {
    DurableSchedulerConfigV1::new(owner_id, Duration::from_secs(5), Duration::ZERO).unwrap()
}

fn one_shot(id: &str, run_at: SystemTime) -> ScheduledTask {
    ScheduledTask {
        id: id.to_string(),
        schedule: TaskSchedule::Once { run_at },
        retry: RetryPolicy::default(),
        priority: 1,
        trace: None,
    }
}

fn wait_until(timeout: Duration, condition: impl Fn() -> bool) -> bool {
    let started = SystemTime::now();
    while started.elapsed().unwrap_or_default() < timeout {
        if condition() {
            return true;
        }
        thread::sleep(Duration::from_millis(2));
    }
    false
}

#[test]
fn terminal_receipt_suppresses_one_shot_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("scheduler.json");
    let task = one_shot("durable-once", SystemTime::now());
    let calls = Arc::new(AtomicUsize::new(0));
    let epochs = Arc::new(Mutex::new(Vec::new()));

    let first = Scheduler::with_state_provider(
        config(),
        durable_config("owner-a"),
        Arc::new(FileSchedulerStateProvider::new(&path).unwrap()),
    )
    .unwrap();
    let callback_calls = Arc::clone(&calls);
    let callback_epochs = Arc::clone(&epochs);
    first
        .schedule_durable(
            task.clone(),
            DurableTaskMisfirePolicyV1::FireOnce,
            Arc::new(move |context| {
                callback_calls.fetch_add(1, Ordering::SeqCst);
                callback_epochs.lock().push(context.fencing_epoch());
                Ok(())
            }),
        )
        .unwrap();
    assert!(wait_until(Duration::from_secs(1), || first
        .snapshot()
        .tasks
        .is_empty()));
    first.shutdown().unwrap();

    let second = Scheduler::with_state_provider(
        config(),
        durable_config("owner-b"),
        Arc::new(FileSchedulerStateProvider::new(path).unwrap()),
    )
    .unwrap();
    second
        .schedule_durable(
            task,
            DurableTaskMisfirePolicyV1::FireOnce,
            Arc::new(|_| panic!("confirmed one-shot ran twice")),
        )
        .unwrap();
    thread::sleep(Duration::from_millis(30));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(*epochs.lock(), vec![Some(1)]);
    assert!(second.snapshot().tasks.is_empty());
}

#[test]
fn persisted_retry_resumes_with_next_attempt_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("scheduler.json");
    let provider = FileSchedulerStateProvider::new(&path).unwrap();
    let mut task = one_shot("durable-retry", SystemTime::now());
    task.retry = RetryPolicy {
        max_attempts: 2,
        initial_backoff: Duration::from_millis(150),
        max_backoff: Duration::from_millis(150),
        multiplier: 1,
        jitter: Duration::ZERO,
    };
    let first = Scheduler::with_state_provider(
        config(),
        durable_config("owner-a"),
        Arc::new(provider.clone()),
    )
    .unwrap();
    first
        .schedule_durable(
            task.clone(),
            DurableTaskMisfirePolicyV1::FireOnce,
            Arc::new(|context| {
                assert_eq!(context.attempt(), 1);
                Err("injected retry".to_string())
            }),
        )
        .unwrap();
    assert!(wait_until(Duration::from_secs(1), || {
        provider
            .record("durable-retry")
            .unwrap()
            .is_some_and(|record| record.attempts == 1 && record.claim.is_none())
    }));
    first.shutdown().unwrap();

    let attempts = Arc::new(Mutex::new(Vec::new()));
    let second = Scheduler::with_state_provider(
        config(),
        durable_config("owner-b"),
        Arc::new(FileSchedulerStateProvider::new(path).unwrap()),
    )
    .unwrap();
    let callback_attempts = Arc::clone(&attempts);
    second
        .schedule_durable(
            task,
            DurableTaskMisfirePolicyV1::FireOnce,
            Arc::new(move |context| {
                callback_attempts.lock().push(context.attempt());
                Ok(())
            }),
        )
        .unwrap();
    assert!(wait_until(Duration::from_secs(1), || attempts.lock().len() == 1));
    assert_eq!(*attempts.lock(), vec![2]);
}

#[test]
fn skip_misfire_commits_without_invoking_callback() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("scheduler.json");
    let task = one_shot(
        "durable-skip",
        SystemTime::now() + Duration::from_millis(40),
    );
    let first = Scheduler::with_state_provider(
        config(),
        durable_config("owner-a"),
        Arc::new(FileSchedulerStateProvider::new(&path).unwrap()),
    )
    .unwrap();
    first
        .schedule_durable(
            task.clone(),
            DurableTaskMisfirePolicyV1::Skip,
            Arc::new(|_| Ok(())),
        )
        .unwrap();
    first.shutdown().unwrap();
    thread::sleep(Duration::from_millis(50));

    let calls = Arc::new(AtomicUsize::new(0));
    let second = Scheduler::with_state_provider(
        config(),
        durable_config("owner-b"),
        Arc::new(FileSchedulerStateProvider::new(path).unwrap()),
    )
    .unwrap();
    let callback_calls = Arc::clone(&calls);
    second
        .schedule_durable(
            task,
            DurableTaskMisfirePolicyV1::Skip,
            Arc::new(move |_| {
                callback_calls.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
        )
        .unwrap();
    assert!(wait_until(Duration::from_secs(1), || second
        .snapshot()
        .tasks
        .is_empty()));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn two_schedulers_execute_one_claim() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("scheduler.json");
    let task = one_shot(
        "durable-shared",
        SystemTime::now() + Duration::from_millis(30),
    );
    let first = Scheduler::with_state_provider(
        config(),
        durable_config("owner-a"),
        Arc::new(FileSchedulerStateProvider::new(&path).unwrap()),
    )
    .unwrap();
    let second = Scheduler::with_state_provider(
        config(),
        durable_config("owner-b"),
        Arc::new(FileSchedulerStateProvider::new(&path).unwrap()),
    )
    .unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    for scheduler in [&first, &second] {
        let callback_calls = Arc::clone(&calls);
        scheduler
            .schedule_durable(
                task.clone(),
                DurableTaskMisfirePolicyV1::FireOnce,
                Arc::new(move |_| {
                    callback_calls.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }),
            )
            .unwrap();
    }
    assert!(wait_until(Duration::from_secs(1), || calls
        .load(Ordering::SeqCst)
        == 1));
    assert!(wait_until(Duration::from_secs(3), || first
        .snapshot()
        .tasks
        .is_empty()
        && second.snapshot().tasks.is_empty()));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn long_callback_keeps_lease_valid_through_renewal() {
    let provider = Arc::new(InMemorySchedulerStateProvider::new());
    let scheduler = Scheduler::with_state_provider(
        SchedulerConfig {
            poll_interval: Duration::from_millis(5),
            ..config()
        },
        DurableSchedulerConfigV1::new("owner-a", Duration::from_millis(40), Duration::ZERO)
            .unwrap(),
        provider,
    )
    .unwrap();
    let cancelled = Arc::new(AtomicBool::new(false));
    let completed = Arc::new(AtomicBool::new(false));
    let callback_cancelled = Arc::clone(&cancelled);
    let callback_completed = Arc::clone(&completed);
    scheduler
        .schedule_durable(
            one_shot("durable-renew", SystemTime::now()),
            DurableTaskMisfirePolicyV1::FireOnce,
            Arc::new(move |context| {
                let started = SystemTime::now();
                while started.elapsed().unwrap_or_default() < Duration::from_millis(120) {
                    if context.is_cancelled() {
                        callback_cancelled.store(true, Ordering::Release);
                    }
                    thread::sleep(Duration::from_millis(2));
                }
                callback_completed.store(true, Ordering::Release);
                Ok(())
            }),
        )
        .unwrap();
    assert!(wait_until(Duration::from_secs(1), || completed.load(Ordering::Acquire)));
    assert!(!cancelled.load(Ordering::Acquire));
    assert!(wait_until(Duration::from_secs(1), || scheduler
        .snapshot()
        .tasks
        .is_empty()));
}

struct FlakyCompletionProvider {
    inner: InMemorySchedulerStateProvider,
    fail_completion_once: AtomicBool,
}

impl SchedulerStateProvider for FlakyCompletionProvider {
    fn register(
        &self,
        registration: &SchedulerStateRegistrationV1,
        max_records: usize,
    ) -> Result<SchedulerStateRecordV1, SchedulerStateError> {
        self.inner.register(registration, max_records)
    }

    fn record(&self, task_id: &str) -> Result<Option<SchedulerStateRecordV1>, SchedulerStateError> {
        self.inner.record(task_id)
    }

    fn try_claim(
        &self,
        request: &SchedulerStateClaimRequestV1,
    ) -> Result<Option<SchedulerStateClaimV1>, SchedulerStateError> {
        self.inner.try_claim(request)
    }

    fn renew_claim(
        &self,
        claim: &SchedulerStateClaimV1,
        now_ms: u64,
        lease_until_ms: u64,
    ) -> Result<(), SchedulerStateError> {
        self.inner.renew_claim(claim, now_ms, lease_until_ms)
    }

    fn complete(
        &self,
        completion: &SchedulerStateCompletionV1,
    ) -> Result<SchedulerStateRecordV1, SchedulerStateError> {
        if self.fail_completion_once.swap(false, Ordering::AcqRel) {
            return Err(SchedulerStateError::Unavailable);
        }
        self.inner.complete(completion)
    }

    fn stats(&self) -> Result<SchedulerStateStatsV1, SchedulerStateError> {
        self.inner.stats()
    }
}

#[test]
fn failed_receipt_commit_retries_without_reinvoking_callback() {
    let provider = Arc::new(FlakyCompletionProvider {
        inner: InMemorySchedulerStateProvider::new(),
        fail_completion_once: AtomicBool::new(true),
    });
    let scheduler =
        Scheduler::with_state_provider(config(), durable_config("owner-a"), provider).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let callback_calls = Arc::clone(&calls);
    scheduler
        .schedule_durable(
            one_shot("durable-pending", SystemTime::now()),
            DurableTaskMisfirePolicyV1::FireOnce,
            Arc::new(move |_| {
                callback_calls.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
        )
        .unwrap();
    assert!(wait_until(Duration::from_secs(1), || scheduler
        .snapshot()
        .tasks
        .is_empty()));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(scheduler.state_provider_error_count() >= 1);
}

#[test]
fn durable_registration_requires_provider_and_exact_definition() {
    let task = one_shot(
        "durable-definition",
        SystemTime::now() + Duration::from_secs(1),
    );
    let ephemeral = Scheduler::new(config()).unwrap();
    assert!(matches!(
        ephemeral.schedule_durable(
            task.clone(),
            DurableTaskMisfirePolicyV1::FireOnce,
            Arc::new(|_| Ok(())),
        ),
        Err(SchedulerError::InvalidConfig(
            "state provider is not configured"
        ))
    ));

    let provider = Arc::new(InMemorySchedulerStateProvider::new());
    let first =
        Scheduler::with_state_provider(config(), durable_config("owner-a"), provider.clone())
            .unwrap();
    first
        .schedule_durable(
            task.clone(),
            DurableTaskMisfirePolicyV1::FireOnce,
            Arc::new(|_| Ok(())),
        )
        .unwrap();
    first.shutdown().unwrap();

    let second =
        Scheduler::with_state_provider(config(), durable_config("owner-b"), provider).unwrap();
    let mut changed = task;
    changed.priority = 2;
    assert!(matches!(
        second.schedule_durable(
            changed,
            DurableTaskMisfirePolicyV1::FireOnce,
            Arc::new(|_| Ok(())),
        ),
        Err(SchedulerError::StateProvider(
            SchedulerStateError::Conflict("durable task definition changed")
        ))
    ));
}
