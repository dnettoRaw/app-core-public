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
use std::collections::HashSet;
use std::sync::atomic::AtomicUsize;

fn test_scheduler(max_tasks: usize, max_concurrent_tasks: usize) -> Scheduler {
    Scheduler::new(SchedulerConfig {
        max_tasks,
        max_concurrent_tasks,
        poll_interval: Duration::from_millis(2),
    })
    .unwrap()
}

#[test]
fn non_cooperative_shutdown_is_bounded_in_subprocess() {
    const CHILD_MARKER: &str = "APPCORE_SCHEDULER_SHUTDOWN_TEST_CHILD";
    if std::env::var_os(CHILD_MARKER).is_some() {
        exercise_non_cooperative_shutdown();
        return;
    }
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "tests::non_cooperative_shutdown_is_bounded_in_subprocess",
            "--nocapture",
        ])
        .env(CHILD_MARKER, "1")
        .stdin(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let started = std::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "shutdown subprocess failed: {status}");
            break;
        }
        if started.elapsed() > Duration::from_secs(15) {
            // Kill only this test's child; a blocked worker must not hang the suite.
            child.kill().unwrap();
            let _ = child.wait();
            panic!("scheduler shutdown subprocess exceeded its external deadline");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn exercise_non_cooperative_shutdown() {
    let scheduler = test_scheduler(2, 1);
    let (started_tx, started_rx) = std::sync::mpsc::sync_channel(1);
    scheduler
        .schedule(
            ScheduledTask {
                id: "never-returns".to_string(),
                schedule: TaskSchedule::Once {
                    run_at: SystemTime::now(),
                },
                retry: RetryPolicy::default(),
                priority: 0,
                trace: None,
            },
            Arc::new(move |_| {
                started_tx.send(()).unwrap();
                loop {
                    thread::park();
                }
            }),
        )
        .unwrap();
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(
        scheduler.shutdown_with_timeout(Duration::from_millis(10)),
        Ok(false)
    );
    let started = std::time::Instant::now();
    assert_eq!(scheduler.shutdown(), Err(SchedulerError::Shutdown));
    assert!(started.elapsed() < Duration::from_secs(8));
    let dropping = std::time::Instant::now();
    drop(scheduler);
    assert!(dropping.elapsed() < Duration::from_secs(1));
    // The test process exits with the parked worker still alive; no safe Rust
    // primitive can force that callback to return or reclaim its captures.
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
fn scheduler_rejects_thread_and_task_counts_above_global_bounds() {
    for config in [
        SchedulerConfig {
            max_tasks: MAX_SCHEDULER_TASKS + 1,
            ..SchedulerConfig::default()
        },
        SchedulerConfig {
            max_concurrent_tasks: MAX_SCHEDULER_WORKERS + 1,
            ..SchedulerConfig::default()
        },
    ] {
        assert!(matches!(
            Scheduler::new(config),
            Err(SchedulerError::InvalidConfig(_))
        ));
    }
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
fn shutdown_deadline_retains_unfinished_worker_until_callback_returns() {
    let scheduler = test_scheduler(2, 1);
    let (started_tx, started_rx) = std::sync::mpsc::sync_channel(1);
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
    let release_rx = Mutex::new(release_rx);
    let task = ScheduledTask {
        id: "shutdown-deadline".to_string(),
        schedule: TaskSchedule::Once {
            run_at: SystemTime::now(),
        },
        retry: RetryPolicy::default(),
        priority: 0,
        trace: None,
    };
    scheduler
        .schedule(
            task.clone(),
            Arc::new(move |_| {
                started_tx.send(()).unwrap();
                // Independent watchdog keeps a broken implementation from hanging tests.
                let _ = release_rx.lock().recv_timeout(Duration::from_secs(3));
                Ok(())
            }),
        )
        .unwrap();
    started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    let started = std::time::Instant::now();
    assert_eq!(
        scheduler.shutdown_with_timeout(Duration::from_millis(10)),
        Ok(false)
    );
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(
        scheduler.schedule(task, Arc::new(|_| Ok(()))).err(),
        Some(SchedulerError::Shutdown)
    );
    assert_eq!(scheduler.shutdown_with_timeout(Duration::ZERO), Ok(false));
    release_tx.send(()).unwrap();
    assert_eq!(
        scheduler.shutdown_with_timeout(Duration::from_secs(1)),
        Ok(true)
    );
    assert_eq!(scheduler.shutdown(), Ok(()));
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
fn worker_threads_remain_fixed_under_load() {
    let scheduler = test_scheduler(128, 4);
    let worker_names = Arc::new(Mutex::new(HashSet::new()));
    let completed = Arc::new(AtomicUsize::new(0));
    for task in 0..64 {
        let worker_names = Arc::clone(&worker_names);
        let completed = Arc::clone(&completed);
        scheduler
            .schedule(
                ScheduledTask {
                    id: format!("fixed-worker-{task}"),
                    schedule: TaskSchedule::Once {
                        run_at: SystemTime::now(),
                    },
                    retry: RetryPolicy::default(),
                    priority: 0,
                    trace: None,
                },
                Arc::new(move |_| {
                    let name = thread::current().name().unwrap_or_default().to_string();
                    worker_names.lock().insert(name);
                    thread::sleep(Duration::from_millis(2));
                    completed.fetch_add(1, Ordering::Release);
                    Ok(())
                }),
            )
            .unwrap();
    }

    assert_eq!(scheduler.worker_thread_count(), 4);
    assert!(wait_until(Duration::from_secs(3), || {
        completed.load(Ordering::Acquire) == 64
    }));
    let names = worker_names.lock();
    assert!(!names.is_empty());
    assert!(names.len() <= scheduler.worker_thread_count());
    assert!(names
        .iter()
        .all(|name| name.starts_with("appcore-scheduler-worker-")));
}

#[test]
fn saturated_queue_is_bounded_observable_and_shutdown_is_cooperative() {
    let scheduler = test_scheduler(16, 1);
    let started = Arc::new(AtomicUsize::new(0));
    for task in 0..8 {
        let started = Arc::clone(&started);
        scheduler
            .schedule(
                ScheduledTask {
                    id: format!("saturated-{task}"),
                    schedule: TaskSchedule::Once {
                        run_at: SystemTime::now(),
                    },
                    retry: RetryPolicy::default(),
                    priority: 0,
                    trace: None,
                },
                Arc::new(move |context| {
                    started.fetch_add(1, Ordering::Release);
                    while !context.is_cancelled() {
                        thread::sleep(Duration::from_millis(1));
                    }
                    Ok(())
                }),
            )
            .unwrap();
    }

    assert!(wait_until(Duration::from_secs(1), || {
        started.load(Ordering::Acquire) == 1 && scheduler.queue_saturation_count() > 0
    }));
    assert!(scheduler.queued_task_count() <= 2);
    scheduler.shutdown().unwrap();
    assert!(scheduler.snapshot().tasks.is_empty());
    assert_eq!(scheduler.snapshot().active_tasks, 0);
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
