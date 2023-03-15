// =============================================================================
//        #######
//     ###       ###     F: supervisor_lifecycle.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/24 13:18:47 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 13:18:47 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Non-blocking reconcile, dependency, watchdog, and shutdown lifecycle.

use super::*;

impl Supervisor {
    /// Reconciles health and schedules restart work without executing it inline.
    pub fn reconcile(&self, timestamp_ms: u64) -> SupervisorResult<()> {
        self.inner.watchdog.record_reconcile_started(timestamp_ms);
        self.apply_restart_completions(timestamp_ms)?;
        for name in self.validate()? {
            self.reconcile_service(&name, timestamp_ms)?;
        }
        self.dispatch_due_restarts(timestamp_ms)?;
        let previous = self.inner.watchdog.state();
        let sequence = self.inner.watchdog.record_reconcile_completed(timestamp_ms);
        self.emit(
            "supervisor",
            SupervisorEventKind::SupervisorProgressed,
            timestamp_ms,
            sequence,
            watchdog_states(previous, WatchdogState::Healthy),
            "reconcile_completed",
        );
        Ok(())
    }

    /// Schedules one restart while enforcing its temporal budget.
    pub fn restart(&self, name: &str, timestamp_ms: u64) -> SupervisorResult<()> {
        let service = self.service(name)?;
        self.require_dependencies(&service)?;
        self.schedule_restart(&service, timestamp_ms)
    }

    /// Stops one enabled service without affecting the host process.
    pub fn stop(&self, name: &str, timestamp_ms: u64) -> SupervisorResult<()> {
        let service = self.service(name)?;
        if !service.descriptor().activation().is_enabled() {
            return Ok(());
        }
        let timeout = service.descriptor().restart_policy().shutdown_timeout;
        match service.stop(timeout) {
            Ok(()) => self.record_stopped(name, timestamp_ms),
            Err(error) => {
                self.record_stop_failure(&service, timestamp_ms)?;
                Err(error)
            }
        }
    }

    /// Independently evaluates watchdog progress and emits transition events.
    pub fn evaluate_watchdog(&self, timestamp_ms: u64) -> WatchdogSnapshot {
        if let Some((previous, next)) = self.inner.watchdog.evaluate(timestamp_ms) {
            let kind = match next {
                WatchdogState::Stalled => SupervisorEventKind::SupervisorStalled,
                WatchdogState::Healthy if previous == WatchdogState::Stalled => {
                    SupervisorEventKind::SupervisorRecovered
                }
                _ => SupervisorEventKind::SupervisorProgressed,
            };
            self.emit(
                "supervisor",
                kind,
                timestamp_ms,
                self.inner.watchdog.reconcile_sequence(),
                watchdog_states(previous, next),
                "watchdog_evaluation",
            );
        }
        self.watchdog_snapshot(timestamp_ms)
    }

    /// Stops the restart executor and all enabled services in reverse order.
    pub fn shutdown(&self, timestamp_ms: u64) -> SupervisorResult<()> {
        self.inner.watchdog.mark_stopping();
        let _ = self
            .inner
            .restart_executor
            .shutdown(Duration::from_secs(10));
        let mut order = self.validate()?;
        order.reverse();
        let mut first_error = None;
        for name in order {
            let service = self.service(&name)?;
            if !service.descriptor().activation().is_enabled() {
                continue;
            }
            let timeout = service.descriptor().restart_policy().shutdown_timeout;
            match service.stop(timeout) {
                Ok(()) => self.record_stopped(&name, timestamp_ms)?,
                Err(error) => {
                    self.record_stop_failure(&service, timestamp_ms)?;
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn reconcile_service(&self, name: &str, timestamp_ms: u64) -> SupervisorResult<()> {
        let service = self.service(name)?;
        if !service.descriptor().activation().is_enabled() {
            return Ok(());
        }
        if self.first_unavailable_dependency(&service)?.is_some() {
            self.update_record(name, ServiceHealth::Degraded, service.runtime_state())?;
            return Ok(());
        }
        let health = service.health();
        let previous = self.update_record(name, health, service.runtime_state())?;
        if health.is_failed() {
            if previous != Some(ServiceHealth::Failed) {
                self.emit(
                    name,
                    SupervisorEventKind::ServiceFailed,
                    timestamp_ms,
                    self.restart_attempt(name),
                    states(previous, health),
                    "health_failed",
                );
            }
            if let Err(error) = self.schedule_restart(&service, timestamp_ms) {
                if !matches!(error, SupervisorError::RestartBudgetExceeded(_)) {
                    return Err(error);
                }
            }
        } else if matches!(
            previous,
            Some(ServiceHealth::Failed | ServiceHealth::Degraded)
        ) {
            self.emit(
                name,
                SupervisorEventKind::ServiceRecovered,
                timestamp_ms,
                self.restart_attempt(name),
                states(previous, health),
                "health_recovered",
            );
        }
        Ok(())
    }

    pub(super) fn require_dependencies(
        &self,
        service: &Arc<dyn ManagedService>,
    ) -> SupervisorResult<()> {
        if let Some(dependency) = self.first_unavailable_dependency(service)? {
            return Err(SupervisorError::DependencyUnavailable {
                service: service.descriptor().name().to_string(),
                dependency,
            });
        }
        Ok(())
    }

    fn first_unavailable_dependency(
        &self,
        service: &Arc<dyn ManagedService>,
    ) -> SupervisorResult<Option<String>> {
        for dependency in service.descriptor().dependencies() {
            let dependency_service = match self.service(dependency.service_id()) {
                Ok(service) => service,
                Err(_) if dependency.requirement() == DependencyRequirement::Optional => continue,
                Err(error) => return Err(error),
            };
            if !dependency
                .requirement()
                .accepts(dependency_service.health())
            {
                return Ok(Some(dependency.service_id().to_string()));
            }
        }
        Ok(None)
    }
}
