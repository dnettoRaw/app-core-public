// =============================================================================
//        #######
//     ###       ###     F: restart_executor.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/24 13:18:47 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 13:24:05 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Bounded restart scheduling and worker execution.

use crate::{ManagedService, ServiceRuntimeState, SupervisorError, SupervisorResult};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub(crate) const DEFAULT_RESTART_QUEUE_CAPACITY: usize = 64;
pub(crate) const DEFAULT_RESTART_WORKERS: usize = 2;

/// Lifecycle state of one scheduled restart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartState {
    /// No restart is pending.
    None,
    /// A restart is waiting for its execution time.
    Scheduled {
        /// Earliest execution time in Unix milliseconds.
        execute_at_ms: u64,
    },
    /// A worker is stopping the previous instance.
    Stopping,
    /// A worker is starting the replacement instance.
    Starting,
    /// A retry is waiting for policy backoff.
    Backoff,
    /// The most recent restart action failed.
    Failed,
}

pub(crate) struct RestartCommand {
    pub service: Arc<dyn ManagedService>,
    pub attempt: u64,
}

pub(crate) enum RestartOutcome {
    Restarted,
    Orphaned,
    Failed,
    Cancelled,
}

pub(crate) struct RestartCompletion {
    pub service_id: String,
    pub attempt: u64,
    pub outcome: RestartOutcome,
}

pub(crate) struct RestartExecutor {
    sender: SyncSender<RestartCommand>,
    completions: Mutex<Receiver<RestartCompletion>>,
    workers: Mutex<Vec<JoinHandle<()>>>,
    cancellation: Arc<AtomicBool>,
    healthy: Arc<AtomicBool>,
    pending: Arc<AtomicU64>,
    queue_capacity: usize,
    worker_count: usize,
}

impl RestartExecutor {
    pub fn new(queue_capacity: usize, worker_count: usize) -> Self {
        let queue_capacity = queue_capacity.max(1);
        let worker_count = worker_count.max(1);
        let (sender, receiver) = mpsc::sync_channel::<RestartCommand>(queue_capacity);
        let (completion_sender, completions) = mpsc::channel();
        let receiver = Arc::new(Mutex::new(receiver));
        let cancellation = Arc::new(AtomicBool::new(false));
        let healthy = Arc::new(AtomicBool::new(true));
        let pending = Arc::new(AtomicU64::new(0));
        let workers = spawn_workers(
            worker_count,
            receiver,
            completion_sender,
            Arc::clone(&cancellation),
            Arc::clone(&healthy),
            Arc::clone(&pending),
        );
        Self {
            sender,
            completions: Mutex::new(completions),
            workers: Mutex::new(workers),
            cancellation,
            healthy,
            pending,
            queue_capacity,
            worker_count,
        }
    }

    pub fn schedule(&self, command: RestartCommand) -> SupervisorResult<()> {
        self.pending.fetch_add(1, Ordering::AcqRel);
        match self.sender.try_send(command) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => {
                self.pending.fetch_sub(1, Ordering::AcqRel);
                Err(SupervisorError::RestartQueueFull)
            }
            Err(TrySendError::Disconnected(_)) => {
                self.pending.fetch_sub(1, Ordering::AcqRel);
                Err(SupervisorError::RestartExecutorStopped)
            }
        }
    }

    pub fn drain_completions(&self) -> Vec<RestartCompletion> {
        let Ok(receiver) = self.completions.lock() else {
            self.healthy.store(false, Ordering::Release);
            return Vec::new();
        };
        receiver.try_iter().collect()
    }

    pub fn snapshot(&self) -> crate::RestartExecutorSnapshot {
        let workers_healthy = self
            .workers
            .lock()
            .map(|workers| workers.iter().all(|worker| !worker.is_finished()))
            .unwrap_or(false);
        crate::RestartExecutorSnapshot {
            healthy: self.healthy.load(Ordering::Acquire)
                && workers_healthy
                && !self.cancellation.load(Ordering::Acquire),
            pending: self.pending.load(Ordering::Acquire),
            queue_capacity: self.queue_capacity,
            worker_count: self.worker_count,
        }
    }

    pub fn shutdown(&self, timeout: Duration) -> bool {
        self.cancellation.store(true, Ordering::Release);
        let deadline = Instant::now().checked_add(timeout);
        while deadline.is_none_or(|deadline| Instant::now() < deadline) {
            let complete = self
                .workers
                .lock()
                .map(|workers| workers.iter().all(JoinHandle::is_finished))
                .unwrap_or(false);
            if complete {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let Ok(mut workers) = self.workers.lock() else {
            self.healthy.store(false, Ordering::Release);
            return false;
        };
        let all_finished = workers.iter().all(JoinHandle::is_finished);
        for worker in workers.drain(..).filter(JoinHandle::is_finished) {
            if worker.join().is_err() {
                self.healthy.store(false, Ordering::Release);
            }
        }
        self.pending.store(0, Ordering::Release);
        self.healthy.store(false, Ordering::Release);
        all_finished
    }
}

fn spawn_workers(
    count: usize,
    receiver: Arc<Mutex<Receiver<RestartCommand>>>,
    completions: mpsc::Sender<RestartCompletion>,
    cancellation: Arc<AtomicBool>,
    healthy: Arc<AtomicBool>,
    pending: Arc<AtomicU64>,
) -> Vec<JoinHandle<()>> {
    (0..count)
        .filter_map(|index| {
            let receiver = Arc::clone(&receiver);
            let completions = completions.clone();
            let cancellation = Arc::clone(&cancellation);
            let healthy = Arc::clone(&healthy);
            let pending = Arc::clone(&pending);
            std::thread::Builder::new()
                .name(format!("appcore-restart-{index}"))
                .spawn(move || restart_worker(receiver, completions, cancellation, pending))
                .map_err(|_| healthy.store(false, Ordering::Release))
                .ok()
        })
        .collect()
}

fn restart_worker(
    receiver: Arc<Mutex<Receiver<RestartCommand>>>,
    completions: mpsc::Sender<RestartCompletion>,
    cancellation: Arc<AtomicBool>,
    pending: Arc<AtomicU64>,
) {
    loop {
        if cancellation.load(Ordering::Acquire) {
            return;
        }
        let command = match receiver
            .lock()
            .map(|receiver| receiver.recv_timeout(Duration::from_millis(25)))
        {
            Ok(Ok(command)) => command,
            Ok(Err(RecvTimeoutError::Timeout)) => continue,
            Ok(Err(RecvTimeoutError::Disconnected)) | Err(_) => return,
        };
        let service_id = command.service.descriptor().name().to_string();
        let attempt = command.attempt;
        let completion = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            execute_restart(command, &cancellation)
        }))
        .unwrap_or(RestartCompletion {
            service_id,
            attempt,
            outcome: RestartOutcome::Failed,
        });
        decrement_pending(&pending);
        let _ = completions.send(completion);
    }
}

fn decrement_pending(pending: &AtomicU64) {
    let _ = pending.fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
        Some(value.saturating_sub(1))
    });
}

fn execute_restart(command: RestartCommand, cancellation: &AtomicBool) -> RestartCompletion {
    let service_id = command.service.descriptor().name().to_string();
    let timeout = command
        .service
        .descriptor()
        .restart_policy()
        .shutdown_timeout;
    let outcome = match command.service.stop(timeout) {
        Err(_) if command.service.runtime_state() == ServiceRuntimeState::Orphaned => {
            RestartOutcome::Orphaned
        }
        Err(_) => RestartOutcome::Failed,
        Ok(()) if cancellation.load(Ordering::Acquire) => RestartOutcome::Cancelled,
        Ok(()) => match command.service.start() {
            Ok(()) => RestartOutcome::Restarted,
            Err(_) => RestartOutcome::Failed,
        },
    };
    RestartCompletion {
        service_id,
        attempt: command.attempt,
        outcome,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CallbackManagedService, ManagedResource, RestartPolicy, ServiceDescriptor};

    #[test]
    fn saturated_queue_does_not_block_executor_shutdown() {
        let stop_started = Arc::new(AtomicBool::new(false));
        let stop_signal = Arc::clone(&stop_started);
        let descriptor =
            ServiceDescriptor::new("worker", ManagedResource::Worker, RestartPolicy::never())
                .unwrap();
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
}
