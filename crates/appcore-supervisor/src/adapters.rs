// =============================================================================
//        #######
//     ###       ###     F: adapters.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/24 11:51:10 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 13:24:05 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Reusable managed-service adapters for threads and embedded resources.

use crate::{
    ManagedService, ServiceActivationState, ServiceDescriptor, ServiceHealth, ServiceRuntimeState,
    SupervisorError, SupervisorResult,
};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

type ThreadFactory =
    dyn Fn(Arc<AtomicBool>) -> Result<JoinHandle<Result<(), String>>, String> + Send + Sync;
type StartAction = dyn Fn() -> Result<(), String> + Send + Sync;
type StopAction = dyn Fn(Duration) -> Result<(), String> + Send + Sync;
type HealthProbe = dyn Fn() -> ServiceHealth + Send + Sync;

struct ThreadRuntime {
    shutdown: Arc<AtomicBool>,
    handle: JoinHandle<Result<(), String>>,
}

struct ThreadState {
    health: ServiceHealth,
    runtime_state: ServiceRuntimeState,
    runtime: Option<ThreadRuntime>,
}

/// Managed adapter for one cooperatively cancellable thread.
pub struct ManagedThreadService {
    descriptor: ServiceDescriptor,
    factory: Arc<ThreadFactory>,
    health_probe: Option<Arc<HealthProbe>>,
    state: Mutex<ThreadState>,
}

impl ManagedThreadService {
    /// Creates a managed thread from a restartable factory.
    pub fn new<F>(descriptor: ServiceDescriptor, factory: F) -> Self
    where
        F: Fn(Arc<AtomicBool>) -> Result<JoinHandle<Result<(), String>>, String>
            + Send
            + Sync
            + 'static,
    {
        Self {
            descriptor,
            factory: Arc::new(factory),
            health_probe: None,
            state: Mutex::new(ThreadState {
                health: ServiceHealth::Unknown,
                runtime_state: ServiceRuntimeState::Stopped,
                runtime: None,
            }),
        }
    }

    /// Adds a live health probe evaluated while the thread is running.
    pub fn with_health_probe<H>(mut self, health_probe: H) -> Self
    where
        H: Fn() -> ServiceHealth + Send + Sync + 'static,
    {
        self.health_probe = Some(Arc::new(health_probe));
        self
    }

    fn refresh(state: &mut ThreadState) {
        let finished = state
            .runtime
            .as_ref()
            .is_some_and(|runtime| runtime.handle.is_finished());
        if !finished {
            return;
        }
        let Some(runtime) = state.runtime.take() else {
            return;
        };
        (state.health, state.runtime_state) = if runtime.shutdown.load(Ordering::Acquire) {
            (ServiceHealth::Unknown, ServiceRuntimeState::Stopped)
        } else {
            (ServiceHealth::Failed, ServiceRuntimeState::Failed)
        };
        let _ = runtime.handle.join();
    }
}

impl ManagedService for ManagedThreadService {
    fn descriptor(&self) -> &ServiceDescriptor {
        &self.descriptor
    }

    fn start(&self) -> SupervisorResult<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| SupervisorError::StatePoisoned)?;
        Self::refresh(&mut state);
        if state.runtime_state == ServiceRuntimeState::Orphaned {
            return Err(SupervisorError::ServiceOrphaned(
                self.descriptor.name().to_string(),
            ));
        }
        if state.runtime.is_some() {
            return Ok(());
        }
        if self.descriptor.activation() != ServiceActivationState::Enabled {
            state.health = ServiceHealth::Unknown;
            state.runtime_state = ServiceRuntimeState::Stopped;
            return Ok(());
        }
        state.health = ServiceHealth::Starting;
        state.runtime_state = ServiceRuntimeState::Starting;
        let shutdown = Arc::new(AtomicBool::new(false));
        let handle = catch_unwind(AssertUnwindSafe(|| (self.factory)(Arc::clone(&shutdown))))
            .map_err(|_| "managed thread factory panicked".to_string())
            .and_then(|result| result)
            .map_err(|reason| {
                state.health = ServiceHealth::Failed;
                state.runtime_state = ServiceRuntimeState::Failed;
                SupervisorError::ServiceFailure {
                    service: self.descriptor.name().to_string(),
                    reason,
                }
            })?;
        state.runtime = Some(ThreadRuntime { shutdown, handle });
        state.health = ServiceHealth::Ready;
        state.runtime_state = ServiceRuntimeState::Running;
        Ok(())
    }

    fn stop(&self, timeout: Duration) -> SupervisorResult<()> {
        let runtime = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| SupervisorError::StatePoisoned)?;
            Self::refresh(&mut state);
            if state.runtime_state == ServiceRuntimeState::Orphaned {
                return Err(SupervisorError::ServiceOrphaned(
                    self.descriptor.name().to_string(),
                ));
            }
            state.health = ServiceHealth::Stopping;
            state.runtime_state = ServiceRuntimeState::StopRequested;
            let Some(runtime) = state.runtime.take() else {
                state.health = ServiceHealth::Unknown;
                state.runtime_state = ServiceRuntimeState::Stopped;
                return Ok(());
            };
            runtime.shutdown.store(true, Ordering::Release);
            state.runtime_state = ServiceRuntimeState::Stopping;
            runtime
        };
        let deadline = Instant::now().checked_add(timeout);
        while !runtime.handle.is_finished()
            && deadline.is_none_or(|deadline| Instant::now() < deadline)
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        if !runtime.handle.is_finished() {
            let mut state = self
                .state
                .lock()
                .map_err(|_| SupervisorError::StatePoisoned)?;
            state.runtime = Some(runtime);
            state.health = ServiceHealth::Failed;
            state.runtime_state = ServiceRuntimeState::Orphaned;
            return Err(SupervisorError::ShutdownTimeout(
                self.descriptor.name().to_string(),
            ));
        }
        let result = runtime
            .handle
            .join()
            .map_err(|_| SupervisorError::ServiceFailure {
                service: self.descriptor.name().to_string(),
                reason: "managed thread panicked".to_string(),
            })?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| SupervisorError::StatePoisoned)?;
        state.health = ServiceHealth::Unknown;
        state.runtime_state = ServiceRuntimeState::Stopped;
        result.map_err(|reason| SupervisorError::ServiceFailure {
            service: self.descriptor.name().to_string(),
            reason,
        })
    }

    fn health(&self) -> ServiceHealth {
        let Ok(mut state) = self.state.lock() else {
            return ServiceHealth::Failed;
        };
        Self::refresh(&mut state);
        if matches!(state.health, ServiceHealth::Ready | ServiceHealth::Healthy) {
            return self
                .health_probe
                .as_ref()
                .map(|probe| {
                    catch_unwind(AssertUnwindSafe(|| probe())).unwrap_or(ServiceHealth::Failed)
                })
                .unwrap_or(state.health);
        }
        state.health
    }

    fn runtime_state(&self) -> ServiceRuntimeState {
        let Ok(mut state) = self.state.lock() else {
            return ServiceRuntimeState::Failed;
        };
        Self::refresh(&mut state);
        state.runtime_state
    }
}

/// Managed adapter for an embedded resource controlled by callbacks.
pub struct CallbackManagedService {
    descriptor: ServiceDescriptor,
    start_action: Arc<StartAction>,
    stop_action: Arc<StopAction>,
    health_probe: Arc<HealthProbe>,
    state: Mutex<CallbackState>,
}

struct CallbackState {
    health: ServiceHealth,
    runtime_state: ServiceRuntimeState,
}

impl CallbackManagedService {
    /// Creates a restartable embedded service.
    pub fn new<S, T, H>(
        descriptor: ServiceDescriptor,
        start_action: S,
        stop_action: T,
        health_probe: H,
    ) -> Self
    where
        S: Fn() -> Result<(), String> + Send + Sync + 'static,
        T: Fn(Duration) -> Result<(), String> + Send + Sync + 'static,
        H: Fn() -> ServiceHealth + Send + Sync + 'static,
    {
        Self {
            descriptor,
            start_action: Arc::new(start_action),
            stop_action: Arc::new(stop_action),
            health_probe: Arc::new(health_probe),
            state: Mutex::new(CallbackState {
                health: ServiceHealth::Unknown,
                runtime_state: ServiceRuntimeState::Stopped,
            }),
        }
    }

    fn set_state(
        &self,
        health: ServiceHealth,
        runtime_state: ServiceRuntimeState,
    ) -> SupervisorResult<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| SupervisorError::StatePoisoned)?;
        state.health = health;
        state.runtime_state = runtime_state;
        Ok(())
    }
}

impl ManagedService for CallbackManagedService {
    fn descriptor(&self) -> &ServiceDescriptor {
        &self.descriptor
    }

    fn start(&self) -> SupervisorResult<()> {
        if self.descriptor.activation() != ServiceActivationState::Enabled {
            return self.set_state(ServiceHealth::Unknown, ServiceRuntimeState::Stopped);
        }
        self.set_state(ServiceHealth::Starting, ServiceRuntimeState::Starting)?;
        catch_unwind(AssertUnwindSafe(|| (self.start_action)()))
            .map_err(|_| "managed start callback panicked".to_string())
            .and_then(|result| result)
            .map_err(|reason| {
                let _ = self.set_state(ServiceHealth::Failed, ServiceRuntimeState::Failed);
                SupervisorError::ServiceFailure {
                    service: self.descriptor.name().to_string(),
                    reason,
                }
            })?;
        self.set_state(ServiceHealth::Ready, ServiceRuntimeState::Running)
    }

    fn stop(&self, timeout: Duration) -> SupervisorResult<()> {
        self.set_state(ServiceHealth::Stopping, ServiceRuntimeState::Stopping)?;
        catch_unwind(AssertUnwindSafe(|| (self.stop_action)(timeout)))
            .map_err(|_| "managed stop callback panicked".to_string())
            .and_then(|result| result)
            .map_err(|reason| {
                let _ = self.set_state(ServiceHealth::Failed, ServiceRuntimeState::Failed);
                SupervisorError::ServiceFailure {
                    service: self.descriptor.name().to_string(),
                    reason,
                }
            })?;
        self.set_state(ServiceHealth::Unknown, ServiceRuntimeState::Stopped)
    }

    fn health(&self) -> ServiceHealth {
        let state = self
            .state
            .lock()
            .map(|state| state.health)
            .unwrap_or(ServiceHealth::Failed);
        if matches!(
            state,
            ServiceHealth::Ready | ServiceHealth::Healthy | ServiceHealth::Degraded
        ) {
            return catch_unwind(AssertUnwindSafe(|| (self.health_probe)()))
                .unwrap_or(ServiceHealth::Failed);
        }
        state
    }

    fn runtime_state(&self) -> ServiceRuntimeState {
        self.state
            .lock()
            .map(|state| state.runtime_state)
            .unwrap_or(ServiceRuntimeState::Failed)
    }
}

/// Managed adapter for a passive Runtime resource with no owned worker.
pub struct PassiveManagedService {
    descriptor: ServiceDescriptor,
    state: Mutex<CallbackState>,
}

impl PassiveManagedService {
    /// Creates a passive service whose start transition produces `Ready`.
    pub fn new(descriptor: ServiceDescriptor) -> Self {
        Self {
            descriptor,
            state: Mutex::new(CallbackState {
                health: ServiceHealth::Unknown,
                runtime_state: ServiceRuntimeState::Stopped,
            }),
        }
    }
}

impl ManagedService for PassiveManagedService {
    fn descriptor(&self) -> &ServiceDescriptor {
        &self.descriptor
    }

    fn start(&self) -> SupervisorResult<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| SupervisorError::StatePoisoned)?;
        if self.descriptor.activation() == ServiceActivationState::Enabled {
            state.health = ServiceHealth::Healthy;
            state.runtime_state = ServiceRuntimeState::Running;
        } else {
            state.health = ServiceHealth::Unknown;
            state.runtime_state = ServiceRuntimeState::Stopped;
        }
        Ok(())
    }

    fn stop(&self, _timeout: Duration) -> SupervisorResult<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| SupervisorError::StatePoisoned)?;
        state.health = ServiceHealth::Unknown;
        state.runtime_state = ServiceRuntimeState::Stopped;
        Ok(())
    }

    fn health(&self) -> ServiceHealth {
        self.state
            .lock()
            .map(|state| state.health)
            .unwrap_or(ServiceHealth::Failed)
    }

    fn runtime_state(&self) -> ServiceRuntimeState {
        self.state
            .lock()
            .map(|state| state.runtime_state)
            .unwrap_or(ServiceRuntimeState::Failed)
    }
}
