// =============================================================================
//        #######
//     ###       ###     F: restart_executor_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/02 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/02 12:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use super::*;
use crate::{CallbackManagedService, ManagedResource, RestartPolicy, ServiceDescriptor};

#[test]
fn saturated_queue_does_not_block_executor_shutdown() {
    let stop_started = Arc::new(AtomicBool::new(false));
    let stop_signal = Arc::clone(&stop_started);
    let descriptor =
        ServiceDescriptor::new("worker", ManagedResource::Worker, RestartPolicy::never()).unwrap();
    let service: Arc<dyn ManagedService> = Arc::new(CallbackManagedService::new(
        descriptor,
        || Ok(()),
        move |_| {
            stop_signal.store(true, Ordering::Release);
            std::thread::sleep(Duration::from_millis(100));
            Ok(())
        },
        || crate::ServiceHealth::Healthy,
    ));
    service.start().unwrap();
    let executor = RestartExecutor::new(1, 1);
    executor
        .schedule(RestartCommand {
            service: Arc::clone(&service),
            attempt: 1,
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    while !stop_started.load(Ordering::Acquire) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(stop_started.load(Ordering::Acquire));
    executor
        .schedule(RestartCommand {
            service: Arc::clone(&service),
            attempt: 2,
        })
        .unwrap();
    assert!(matches!(
        executor.schedule(RestartCommand {
            service,
            attempt: 3,
        }),
        Err(SupervisorError::RestartQueueFull)
    ));

    assert!(executor.shutdown(Duration::from_secs(1)));
    assert_eq!(executor.snapshot().pending, 0);
}

#[test]
fn pending_counter_saturates_after_late_worker_completion() {
    let pending = AtomicU64::new(0);
    decrement_pending(&pending);
    assert_eq!(pending.load(Ordering::Acquire), 0);
}

#[test]
fn stalled_completion_drain_is_bounded_and_shutdown_releases_commands() {
    struct CountingService {
        descriptor: ServiceDescriptor,
        stops: Arc<AtomicU64>,
    }

    impl ManagedService for CountingService {
        fn descriptor(&self) -> &ServiceDescriptor {
            &self.descriptor
        }

        fn start(&self) -> SupervisorResult<()> {
            Ok(())
        }

        fn stop(&self, _timeout: Duration) -> SupervisorResult<()> {
            self.stops.fetch_add(1, Ordering::AcqRel);
            Ok(())
        }

        fn health(&self) -> crate::ServiceHealth {
            crate::ServiceHealth::Healthy
        }
    }

    let stops = Arc::new(AtomicU64::new(0));
    let service: Arc<dyn ManagedService> = Arc::new(CountingService {
        descriptor: ServiceDescriptor::new(
            "completion-pressure",
            ManagedResource::Worker,
            RestartPolicy::never(),
        )
        .unwrap(),
        stops: Arc::clone(&stops),
    });
    let executor = RestartExecutor::new(1, 1);
    for attempt in 1..=3 {
        executor
            .schedule(RestartCommand {
                service: Arc::clone(&service),
                attempt,
            })
            .unwrap();
        wait_for_count(&stops, attempt);
    }
    executor
        .schedule(RestartCommand {
            service: Arc::clone(&service),
            attempt: 4,
        })
        .unwrap();
    assert!(matches!(
        executor.schedule(RestartCommand {
            service: Arc::clone(&service),
            attempt: 5,
        }),
        Err(SupervisorError::RestartQueueFull)
    ));
    assert_eq!(executor.snapshot().pending, 2);

    assert!(executor.shutdown(Duration::from_secs(1)));
    assert_eq!(executor.snapshot().pending, 0);
    assert!(executor.drain_completions().is_empty());
    assert_eq!(Arc::strong_count(&service), 1);
    assert!(matches!(
        executor.schedule(RestartCommand {
            service,
            attempt: 6,
        }),
        Err(SupervisorError::RestartExecutorStopped)
    ));
}

fn wait_for_count(counter: &AtomicU64, expected: u64) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while counter.load(Ordering::Acquire) < expected && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(counter.load(Ordering::Acquire), expected);
}

#[test]
fn managed_service_panic_does_not_kill_restart_worker() {
    struct PanicService {
        descriptor: ServiceDescriptor,
    }

    impl ManagedService for PanicService {
        fn descriptor(&self) -> &ServiceDescriptor {
            &self.descriptor
        }

        fn start(&self) -> SupervisorResult<()> {
            Ok(())
        }

        fn stop(&self, _timeout: Duration) -> SupervisorResult<()> {
            panic!("injected managed-service panic");
        }

        fn health(&self) -> crate::ServiceHealth {
            crate::ServiceHealth::Failed
        }
    }

    let service: Arc<dyn ManagedService> = Arc::new(PanicService {
        descriptor: ServiceDescriptor::new(
            "panic-worker",
            ManagedResource::Worker,
            RestartPolicy::never(),
        )
        .unwrap(),
    });
    let executor = RestartExecutor::new(1, 1);
    executor
        .schedule(RestartCommand {
            service,
            attempt: 1,
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    let completion = loop {
        if let Some(completion) = executor.drain_completions().pop() {
            break completion;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    };
    assert!(matches!(completion.outcome, RestartOutcome::Failed));
    assert_eq!(executor.snapshot().pending, 0);
    assert!(executor.shutdown(Duration::from_secs(1)));
}
