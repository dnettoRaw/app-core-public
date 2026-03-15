// =============================================================================
//        #######
//     ###       ###     F: runtime_services_monitor.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/24 13:18:47 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 13:18:47 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Independent Supervisor reconciliation and watchdog monitor loops.

use super::{now_ms, supervisor_error, BootstrapError};
use appcore_core::RuntimeOperationalMode;
use appcore_supervisor::{Supervisor, WatchdogState};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub(super) fn start_supervisor_monitor(
    supervisor: Supervisor,
    shutdown: Arc<AtomicBool>,
) -> Result<JoinHandle<Result<(), BootstrapError>>, BootstrapError> {
    thread::Builder::new()
        .name("appcore-service-supervisor".to_string())
        .spawn(move || {
            while !shutdown.load(Ordering::Acquire) {
                if let Err(error) = supervisor.reconcile(now_ms()) {
                    supervisor.watchdog().mark_failed();
                    shutdown.store(true, Ordering::Release);
                    return Err(supervisor_error(error));
                }
                thread::sleep(Duration::from_millis(100));
            }
            Ok(())
        })
        .map_err(|error| {
            BootstrapError::Runtime(format!("failed to start service supervisor: {error}"))
        })
}

pub(super) fn start_watchdog_monitor(
    supervisor: Supervisor,
    shutdown: Arc<AtomicBool>,
    operation_mode: Arc<Mutex<RuntimeOperationalMode>>,
) -> Result<JoinHandle<Result<(), BootstrapError>>, BootstrapError> {
    let interval = Duration::from_millis(supervisor.watchdog().config().check_interval_ms.max(1));
    thread::Builder::new()
        .name("appcore-supervisor-watchdog".to_string())
        .spawn(move || {
            while !shutdown.load(Ordering::Acquire) {
                let snapshot = supervisor.evaluate_watchdog(now_ms());
                if matches!(
                    snapshot.state,
                    WatchdogState::Stalled | WatchdogState::Failed
                ) {
                    set_runtime_degraded(&operation_mode);
                }
                wait_for_monitor_interval(&shutdown, interval);
            }
            Ok(())
        })
        .map_err(|error| BootstrapError::Runtime(format!("failed to start watchdog: {error}")))
}

pub(super) fn join_monitor(
    monitor: Option<JoinHandle<Result<(), BootstrapError>>>,
    name: &str,
) -> Result<(), BootstrapError> {
    let Some(monitor) = monitor else {
        return Ok(());
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while !monitor.is_finished() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    if !monitor.is_finished() {
        return Err(BootstrapError::Runtime(format!(
            "{name} monitor did not stop before its deadline"
        )));
    }
    monitor
        .join()
        .map_err(|_| BootstrapError::Runtime(format!("{name} monitor panicked")))?
}

fn set_runtime_degraded(operation_mode: &Mutex<RuntimeOperationalMode>) {
    let mut mode = operation_mode.lock();
    if matches!(
        *mode,
        RuntimeOperationalMode::ReadWrite | RuntimeOperationalMode::ReadOnly
    ) {
        *mode = RuntimeOperationalMode::Degraded;
    }
}

fn wait_for_monitor_interval(shutdown: &AtomicBool, interval: Duration) {
    let deadline = Instant::now() + interval;
    while !shutdown.load(Ordering::Acquire) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return;
        }
        thread::sleep(remaining.min(Duration::from_millis(50)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stalled_watchdog_degrades_runtime_operation() {
        let mode = Mutex::new(RuntimeOperationalMode::ReadWrite);
        set_runtime_degraded(&mode);
        assert_eq!(*mode.lock(), RuntimeOperationalMode::Degraded);
    }
}
