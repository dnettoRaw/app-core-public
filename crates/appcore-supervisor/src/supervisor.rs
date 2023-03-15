// =============================================================================
//        #######
//     ###       ###     F: supervisor.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/24 11:51:10 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 13:18:47 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Process-local managed-service orchestration.

use crate::graph::{topological_order, validate_dependencies};
use crate::restart_executor::{
    RestartCommand, RestartCompletion, RestartExecutor, RestartOutcome,
    DEFAULT_RESTART_QUEUE_CAPACITY, DEFAULT_RESTART_WORKERS,
};
use crate::{
    DependencyRequirement, ManagedService, RestartMode, RestartState, ServiceHealth,
    ServiceRuntimeState, SupervisorDiagnosis, SupervisorError, SupervisorEvent,
    SupervisorEventKind, SupervisorResult, SupervisorWatchdog, WatchdogConfig, WatchdogSnapshot,
    WatchdogState, DEFAULT_EVENT_CAPACITY,
};
use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[path = "supervisor_diagnostics.rs"]
mod diagnostics;
#[path = "supervisor_lifecycle.rs"]
mod lifecycle;
#[path = "supervisor_restart.rs"]
mod restart;

pub(super) struct RuntimeRecord {
    health: Option<ServiceHealth>,
    runtime_state: ServiceRuntimeState,
    restart_state: RestartState,
    restart_times_ms: VecDeque<u64>,
    restart_count: u64,
    operator_required: bool,
    quarantined: bool,
}

impl Default for RuntimeRecord {
    fn default() -> Self {
        Self {
            health: None,
            runtime_state: ServiceRuntimeState::Stopped,
            restart_state: RestartState::None,
            restart_times_ms: VecDeque::new(),
            restart_count: 0,
            operator_required: false,
            quarantined: false,
        }
    }
}

struct SupervisorInner {
    services: RwLock<BTreeMap<String, Arc<dyn ManagedService>>>,
    records: Mutex<BTreeMap<String, RuntimeRecord>>,
    events: Mutex<VecDeque<SupervisorEvent>>,
    event_capacity: usize,
    event_sequence: AtomicU64,
    jitter_state: AtomicU64,
    watchdog: Arc<SupervisorWatchdog>,
    restart_executor: RestartExecutor,
}

/// Process-local orchestrator for Runtime-owned managed services.
///
/// The supervisor never starts, stops, or replaces its host process.
#[derive(Clone)]
pub struct Supervisor {
    inner: Arc<SupervisorInner>,
}

impl Supervisor {
    /// Creates an empty supervisor with safe watchdog and executor defaults.
    pub fn new() -> Self {
        let created_at_ms = now_ms();
        Self::assemble(
            DEFAULT_EVENT_CAPACITY,
            created_at_ms,
            SupervisorWatchdog::with_default(created_at_ms),
        )
    }

    /// Creates an empty supervisor with installation watchdog policy.
    pub fn with_watchdog_config(config: WatchdogConfig) -> SupervisorResult<Self> {
        Self::with_options(DEFAULT_EVENT_CAPACITY, config, now_ms())
    }

    /// Creates an empty supervisor with an explicit event bound.
    pub fn with_event_capacity(event_capacity: usize) -> Self {
        let created_at_ms = now_ms();
        Self::assemble(
            event_capacity,
            created_at_ms,
            SupervisorWatchdog::with_default(created_at_ms),
        )
    }

    fn with_options(
        event_capacity: usize,
        watchdog_config: WatchdogConfig,
        created_at_ms: u64,
    ) -> SupervisorResult<Self> {
        let watchdog = SupervisorWatchdog::new(watchdog_config, created_at_ms)?;
        Ok(Self::assemble(event_capacity, created_at_ms, watchdog))
    }

    fn assemble(event_capacity: usize, created_at_ms: u64, watchdog: SupervisorWatchdog) -> Self {
        Self {
            inner: Arc::new(SupervisorInner {
                services: RwLock::new(BTreeMap::new()),
                records: Mutex::new(BTreeMap::new()),
                events: Mutex::new(VecDeque::new()),
                event_capacity: event_capacity.max(1),
                event_sequence: AtomicU64::new(0),
                jitter_state: AtomicU64::new(created_at_ms.max(1)),
                watchdog: Arc::new(watchdog),
                restart_executor: RestartExecutor::new(
                    DEFAULT_RESTART_QUEUE_CAPACITY,
                    DEFAULT_RESTART_WORKERS,
                ),
            }),
        }
    }

    /// Reports whether two handles reference the same supervisor instance.
    pub fn same_instance(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    /// Returns the lock-independent watchdog shared with health consumers.
    pub fn watchdog(&self) -> Arc<SupervisorWatchdog> {
        Arc::clone(&self.inner.watchdog)
    }

    /// Registers one unique service without starting it.
    pub fn register(&self, service: Arc<dyn ManagedService>) -> SupervisorResult<()> {
        service.descriptor().validate()?;
        let name = service.descriptor().name().to_string();
        let mut services = self
            .inner
            .services
            .write()
            .map_err(|_| SupervisorError::StatePoisoned)?;
        if services.contains_key(&name) {
            return Err(SupervisorError::ServiceAlreadyRegistered(name));
        }
        services.insert(name.clone(), service);
        self.inner
            .records
            .lock()
            .map_err(|_| SupervisorError::StatePoisoned)?
            .insert(name, RuntimeRecord::default());
        Ok(())
    }

    /// Registers a service or replaces an inactive deployment placeholder.
    ///
    /// An enabled registration is never replaced because it may own resources.
    pub fn register_or_replace_inactive(
        &self,
        service: Arc<dyn ManagedService>,
    ) -> SupervisorResult<()> {
        service.descriptor().validate()?;
        let name = service.descriptor().name().to_string();
        let mut services = self
            .inner
            .services
            .write()
            .map_err(|_| SupervisorError::StatePoisoned)?;
        if services
            .get(&name)
            .is_some_and(|current| current.descriptor().activation().is_enabled())
        {
            return Err(SupervisorError::ServiceAlreadyRegistered(name));
        }
        services.insert(name.clone(), service);
        self.inner
            .records
            .lock()
            .map_err(|_| SupervisorError::StatePoisoned)?
            .insert(name, RuntimeRecord::default());
        Ok(())
    }

    /// Validates policies and returns dependency-first service order.
    pub fn validate(&self) -> SupervisorResult<Vec<String>> {
        let services = self
            .inner
            .services
            .read()
            .map_err(|_| SupervisorError::StatePoisoned)?;
        validate_dependencies(&services)?;
        topological_order(&services)
    }

    /// Starts all enabled services in dependency order.
    pub fn start_all(&self) -> SupervisorResult<()> {
        for name in self.validate()? {
            let service = self.service(&name)?;
            if service.descriptor().activation().is_enabled() {
                self.start(&name, now_ms())?;
            }
        }
        Ok(())
    }

    /// Starts one enabled service after checking all dependencies.
    pub fn start(&self, name: &str, timestamp_ms: u64) -> SupervisorResult<()> {
        let service = self.service(name)?;
        if !service.descriptor().activation().is_enabled() {
            return Ok(());
        }
        self.require_dependencies(&service)?;
        let previous = self.record_health(name)?;
        match service.start() {
            Ok(()) => {
                let health = service.health();
                self.update_record(name, health, service.runtime_state())?;
                self.emit(
                    name,
                    SupervisorEventKind::ServiceStarted,
                    timestamp_ms,
                    0,
                    states(previous, health),
                    "lifecycle_start",
                );
                Ok(())
            }
            Err(error) => {
                self.update_record(name, ServiceHealth::Failed, service.runtime_state())?;
                self.emit(
                    name,
                    SupervisorEventKind::ServiceFailed,
                    timestamp_ms,
                    0,
                    states(previous, ServiceHealth::Failed),
                    "start_failed",
                );
                Err(error)
            }
        }
    }

    pub(super) fn service(&self, name: &str) -> SupervisorResult<Arc<dyn ManagedService>> {
        self.inner
            .services
            .read()
            .map_err(|_| SupervisorError::StatePoisoned)?
            .get(name)
            .cloned()
            .ok_or_else(|| SupervisorError::ServiceNotFound(name.to_string()))
    }

    pub(super) fn records(
        &self,
    ) -> SupervisorResult<std::sync::MutexGuard<'_, BTreeMap<String, RuntimeRecord>>> {
        self.inner
            .records
            .lock()
            .map_err(|_| SupervisorError::StatePoisoned)
    }

    pub(super) fn update_record(
        &self,
        name: &str,
        health: ServiceHealth,
        runtime_state: ServiceRuntimeState,
    ) -> SupervisorResult<Option<ServiceHealth>> {
        let mut records = self.records()?;
        let record = record_mut(&mut records, name)?;
        record.runtime_state = runtime_state;
        Ok(record.health.replace(health))
    }

    fn record_health(&self, name: &str) -> SupervisorResult<Option<ServiceHealth>> {
        let records = self.records()?;
        records
            .get(name)
            .map(|record| record.health)
            .ok_or_else(|| SupervisorError::ServiceNotFound(name.to_string()))
    }

    pub(super) fn restart_attempt(&self, name: &str) -> u64 {
        self.inner
            .records
            .lock()
            .ok()
            .and_then(|records| records.get(name).map(|record| record.restart_count))
            .unwrap_or(0)
    }

    pub(super) fn emit(
        &self,
        service: &str,
        kind: SupervisorEventKind,
        timestamp_ms: u64,
        attempt: u64,
        transition: (&str, &str),
        reason: &str,
    ) {
        let trace = self
            .inner
            .event_sequence
            .fetch_add(1, Ordering::AcqRel)
            .saturating_add(1);
        let Ok(mut events) = self.inner.events.lock() else {
            return;
        };
        while events.len() >= self.inner.event_capacity {
            events.pop_front();
        }
        events.push_back(SupervisorEvent::new(
            service,
            kind,
            timestamp_ms,
            attempt,
            transition,
            reason,
            format!("supervisor-{trace}"),
        ));
    }

    pub(super) fn jitter(&self, maximum: Duration) -> Duration {
        let maximum_ms = duration_ms(maximum);
        if maximum_ms == 0 {
            return Duration::ZERO;
        }
        let mut current = self.inner.jitter_state.load(Ordering::Relaxed);
        loop {
            let mut next = current;
            next ^= next << 13;
            next ^= next >> 7;
            next ^= next << 17;
            match self.inner.jitter_state.compare_exchange_weak(
                current,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Duration::from_millis(next % maximum_ms.saturating_add(1)),
                Err(observed) => current = observed,
            }
        }
    }
}

impl Default for Supervisor {
    fn default() -> Self {
        Self::new()
    }
}

pub(super) fn record_mut<'a>(
    records: &'a mut BTreeMap<String, RuntimeRecord>,
    name: &str,
) -> SupervisorResult<&'a mut RuntimeRecord> {
    records
        .get_mut(name)
        .ok_or_else(|| SupervisorError::ServiceNotFound(name.to_string()))
}

pub(super) fn states(
    previous: Option<ServiceHealth>,
    next: ServiceHealth,
) -> (&'static str, &'static str) {
    (
        previous.map(health_name).unwrap_or("Unknown"),
        health_name(next),
    )
}

pub(super) fn health_name(health: ServiceHealth) -> &'static str {
    match health {
        ServiceHealth::Ready => "Ready",
        ServiceHealth::Healthy => "Healthy",
        ServiceHealth::Degraded => "Degraded",
        ServiceHealth::Failed => "Failed",
        ServiceHealth::Starting => "Starting",
        ServiceHealth::Stopping => "Stopping",
        ServiceHealth::Unknown => "Unknown",
    }
}

pub(super) fn watchdog_states(
    previous: WatchdogState,
    next: WatchdogState,
) -> (&'static str, &'static str) {
    (watchdog_name(previous), watchdog_name(next))
}

fn watchdog_name(state: WatchdogState) -> &'static str {
    match state {
        WatchdogState::Starting => "Starting",
        WatchdogState::Healthy => "Healthy",
        WatchdogState::Stalled => "Stalled",
        WatchdogState::Failed => "Failed",
        WatchdogState::Stopping => "Stopping",
    }
}

pub(super) fn duration_ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

pub(super) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "supervisor_tests.rs"]
mod tests;
