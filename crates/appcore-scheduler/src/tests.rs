// =============================================================================
//        #######
//     ###       ###     F: tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 13:24:05 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;
use std::sync::atomic::AtomicUsize;

fn test_scheduler(max_tasks: usize, max_concurrent_tasks: usize) -> Scheduler {
    Scheduler::new(SchedulerConfig {
        max_tasks,
        max_concurrent_tasks,
        poll_interval: Duration::from_millis(2),
    })
    .unwrap()
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
fn one_shot_task_runs_once() {
    let scheduler = test_scheduler(4, 1);
    let calls = Arc::new(AtomicUsize::new(0));
    let callback_calls = Arc::clone(&calls);
    scheduler
        .schedule(
            ScheduledTask {
                id: "once".to_string(),
                schedule: TaskSchedule::Once {
                    run_at: SystemTime::now(),
                },
                retry: RetryPolicy::default(),
                priority: 1,
                trace: None,
            },
            Arc::new(move |_| {
                callback_calls.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
        )
        .unwrap();
    assert!(wait_until(Duration::from_secs(1), || calls
        .load(Ordering::SeqCst)
        == 1));
    assert!(wait_until(Duration::from_secs(1), || scheduler
        .snapshot()
        .tasks
        .is_empty()));
}

#[test]
fn scheduler_propagates_trace_to_task_context() {
    let scheduler = test_scheduler(2, 1);
    let observed = Arc::new(Mutex::new(None));
    let callback_observed = Arc::clone(&observed);
    let trace = TraceContext::new(
        "trace-scheduler",
        "span-scheduler",
        appcore_core::CoreId::new("core-a").unwrap(),
        appcore_core::CoreId::new("core-a").unwrap(),
        appcore_core::TenantId::new("tenant-a").unwrap(),
    )
    .unwrap();
    scheduler
        .schedule(
            ScheduledTask {
                id: "traced".to_string(),
                schedule: TaskSchedule::Once {
                    run_at: SystemTime::now(),
                },
                retry: RetryPolicy::default(),
                priority: 1,
                trace: Some(trace),
            },
            Arc::new(move |context| {
                *callback_observed.lock() = context.trace().map(|trace| trace.trace_id.clone());
                Ok(())
            }),
        )
        .unwrap();

    assert!(wait_until(Duration::from_secs(1), || observed
        .lock()
        .is_some()));
    assert_eq!(observed.lock().as_deref(), Some("trace-scheduler"));
}

#[test]
fn cancellation_prevents_execution() {
    let scheduler = test_scheduler(4, 1);
    let calls = Arc::new(AtomicUsize::new(0));
    let callback_calls = Arc::clone(&calls);
    let handle = scheduler
        .schedule(
            ScheduledTask {
                id: "cancelled".to_string(),
                schedule: TaskSchedule::Once {
                    run_at: SystemTime::now() + Duration::from_millis(100),
                },
                retry: RetryPolicy::default(),
                priority: 1,
                trace: None,
            },
            Arc::new(move |_| {
                callback_calls.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
        )
        .unwrap();
    handle.cancel();
    thread::sleep(Duration::from_millis(150));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn interval_can_be_cancelled() {
    let scheduler = test_scheduler(4, 1);
    let calls = Arc::new(AtomicUsize::new(0));
    let callback_calls = Arc::clone(&calls);
    let handle = scheduler
        .schedule(
            ScheduledTask {
                id: "interval".to_string(),
                schedule: TaskSchedule::Interval {
                    every: Duration::from_millis(5),
                    start_at: Some(SystemTime::now()),
                },
                retry: RetryPolicy::default(),
                priority: 1,
                trace: None,
            },
            Arc::new(move |_| {
                callback_calls.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
        )
        .unwrap();
    assert!(wait_until(Duration::from_secs(1), || calls
        .load(Ordering::SeqCst)
        >= 2));
    handle.cancel();
    let stopped_at = calls.load(Ordering::SeqCst);
    thread::sleep(Duration::from_millis(30));
    assert_eq!(calls.load(Ordering::SeqCst), stopped_at);
}

#[test]
fn failed_task_retries_with_attempt_number() {
    let scheduler = test_scheduler(4, 1);
    let attempts = Arc::new(Mutex::new(Vec::new()));
    let callback_attempts = Arc::clone(&attempts);
    scheduler
        .schedule(
            ScheduledTask {
                id: "retry".to_string(),
                schedule: TaskSchedule::Once {
                    run_at: SystemTime::now(),
                },
                retry: RetryPolicy {
                    max_attempts: 3,
                    initial_backoff: Duration::from_millis(1),
                    max_backoff: Duration::from_millis(2),
                    multiplier: 2,
                    jitter: Duration::ZERO,
                },
                priority: 1,
                trace: None,
            },
            Arc::new(move |context| {
                callback_attempts.lock().push(context.attempt());
                if context.attempt() < 3 {
                    Err("token=must-not-leak".to_string())
                } else {
                    Ok(())
                }
            }),
        )
        .unwrap();
    assert!(wait_until(Duration::from_secs(1), || attempts.lock().len() == 3));
    assert_eq!(*attempts.lock(), vec![1, 2, 3]);
}

#[test]
fn priority_orders_simultaneously_due_tasks() {
    let scheduler = test_scheduler(4, 1);
    let order = Arc::new(Mutex::new(Vec::new()));
    let run_at = SystemTime::now() + Duration::from_millis(30);
    for (id, priority) in [("low", 1), ("high", 10)] {
        let callback_order = Arc::clone(&order);
        scheduler
            .schedule(
                ScheduledTask {
                    id: id.to_string(),
                    schedule: TaskSchedule::Once { run_at },
                    retry: RetryPolicy::default(),
                    priority,
                    trace: None,
                },
                Arc::new(move |context| {
                    callback_order.lock().push(context.task_id().to_string());
                    Ok(())
                }),
            )
            .unwrap();
    }
    assert!(wait_until(Duration::from_secs(1), || order.lock().len() == 2));
    assert_eq!(*order.lock(), vec!["high".to_string(), "low".to_string()]);
}

#[test]
fn task_limit_and_duplicate_ids_are_enforced() {
    let scheduler = test_scheduler(1, 1);
    let task = |id: &str| ScheduledTask {
        id: id.to_string(),
        schedule: TaskSchedule::Once {
            run_at: SystemTime::now() + Duration::from_secs(60),
        },
        retry: RetryPolicy::default(),
        priority: 0,
        trace: None,
    };
    scheduler
        .schedule(task("one"), Arc::new(|_| Ok(())))
        .unwrap();
    assert!(matches!(
        scheduler.schedule(task("one"), Arc::new(|_| Ok(()))),
        Err(SchedulerError::DuplicateTask(id)) if id == "one"
    ));
    assert!(matches!(
        scheduler.schedule(task("two"), Arc::new(|_| Ok(()))),
        Err(SchedulerError::CapacityExceeded { max_tasks: 1 })
    ));
}

#[test]
fn cron_is_validated_and_shutdown_is_idempotent() {
    let scheduler = test_scheduler(2, 1);
    scheduler
        .schedule(
            ScheduledTask {
                id: "cron".to_string(),
                schedule: TaskSchedule::Cron {
                    expression: "0 * * * * *".to_string(),
                },
                retry: RetryPolicy::default(),
                priority: 0,
                trace: None,
            },
            Arc::new(|_| Ok(())),
        )
        .unwrap();
    assert_eq!(scheduler.snapshot().tasks.len(), 1);
    assert_eq!(scheduler.shutdown(), Ok(()));
    assert_eq!(scheduler.shutdown(), Ok(()));
    assert!(scheduler.snapshot().shutdown);
}

#[test]
fn panic_is_isolated_and_retried() {
    let scheduler = test_scheduler(2, 1);
    let calls = Arc::new(AtomicUsize::new(0));
    let callback_calls = Arc::clone(&calls);
    scheduler
        .schedule(
            ScheduledTask {
                id: "panic".to_string(),
                schedule: TaskSchedule::Once {
                    run_at: SystemTime::now(),
                },
                retry: RetryPolicy {
                    max_attempts: 2,
                    initial_backoff: Duration::from_millis(1),
                    max_backoff: Duration::from_millis(1),
                    multiplier: 1,
                    jitter: Duration::ZERO,
                },
                priority: 0,
                trace: None,
            },
            Arc::new(move |_| {
                if callback_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    panic!("injected panic");
                }
                Ok(())
            }),
        )
        .unwrap();
    assert!(wait_until(Duration::from_secs(1), || calls
        .load(Ordering::SeqCst)
        == 2));
}

#[test]
fn extreme_durations_and_post_shutdown_scheduling_fail_without_panicking() {
    let scheduler = test_scheduler(2, 1);
    let interval = ScheduledTask {
        id: "clock-overflow".to_string(),
        schedule: TaskSchedule::Interval {
            every: Duration::MAX,
            start_at: None,
        },
        retry: RetryPolicy::default(),
        priority: 0,
        trace: None,
    };
    assert!(matches!(
        scheduler.schedule(interval, Arc::new(|_| Ok(()))),
        Err(SchedulerError::InvalidSchedule(
            "interval exceeds clock range"
        ))
    ));

    scheduler.shutdown().unwrap();
    let once = ScheduledTask {
        id: "after-shutdown".to_string(),
        schedule: TaskSchedule::Once {
            run_at: SystemTime::now(),
        },
        retry: RetryPolicy::default(),
        priority: 0,
        trace: None,
    };
    assert!(matches!(
        scheduler.schedule(once, Arc::new(|_| Ok(()))),
        Err(SchedulerError::Shutdown)
    ));
}
