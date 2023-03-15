// =============================================================================
//        #######
//     ###       ###     F: supervisor_diagnostics.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/24 13:18:47 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 13:18:47 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Immutable diagnosis, health aggregation, and stop-state recording.

use super::*;

impl Supervisor {
    /// Returns a deterministic diagnostic report without mutating services.
    pub fn diagnose(&self) -> SupervisorDiagnosis {
        let validation = self.validate();
        SupervisorDiagnosis {
            graph_valid: validation.is_ok(),
            issues: validation
                .err()
                .map(|error| vec![error.to_string()])
                .unwrap_or_default(),
            services: self.snapshots(),
            watchdog: self.watchdog_snapshot(now_ms()),
            restart_executor: self.inner.restart_executor.snapshot(),
        }
    }

    /// Returns current service snapshots in lexical order.
    pub fn snapshots(&self) -> Vec<crate::ServiceSnapshot> {
        let Ok(services) = self.inner.services.read() else {
            return Vec::new();
        };
        let Ok(records) = self.inner.records.lock() else {
            return Vec::new();
        };
        services
            .iter()
            .map(|(name, service)| service_snapshot(name, service, records.get(name)))
            .collect()
    }

    /// Returns retained lifecycle events without removing them.
    pub fn events(&self) -> Vec<SupervisorEvent> {
        self.inner
            .events
            .lock()
            .map(|events| events.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Reports whether graph, watchdog, executor, and critical services are healthy.
    pub fn is_healthy(&self, timestamp_ms: u64) -> bool {
        self.validate().is_ok()
            && self.watchdog_snapshot(timestamp_ms).is_healthy()
            && self.inner.restart_executor.snapshot().healthy
            && self.critical_services_healthy()
    }

    pub(super) fn critical_services_healthy(&self) -> bool {
        self.snapshots().iter().all(|service| {
            !service.enabled
                || !service.critical
                || matches!(
                    service.health,
                    ServiceHealth::Ready | ServiceHealth::Healthy
                ) && !service.quarantined
        })
    }

    pub(super) fn watchdog_snapshot(&self, timestamp_ms: u64) -> WatchdogSnapshot {
        let watchdog = &self.inner.watchdog;
        WatchdogSnapshot {
            state: watchdog.state(),
            last_reconcile_at_ms: watchdog.last_reconcile_at_ms(),
            last_progress_at_ms: watchdog.last_progress_at_ms(),
            reconcile_sequence: watchdog.reconcile_sequence(),
            stalled_for_ms: watchdog.stalled_for_ms(timestamp_ms),
            critical_services_healthy: self.critical_services_healthy(),
            enabled: watchdog.config().enabled,
            stall_timeout_ms: watchdog.config().stall_timeout_ms,
        }
    }

    pub(super) fn record_stopped(&self, name: &str, timestamp_ms: u64) -> SupervisorResult<()> {
        self.update_record(name, ServiceHealth::Unknown, ServiceRuntimeState::Stopped)?;
        self.emit(
            name,
            SupervisorEventKind::ServiceStopped,
            timestamp_ms,
            self.restart_attempt(name),
            ("Running", "Stopped"),
            "lifecycle_stop",
        );
        Ok(())
    }

    pub(super) fn record_stop_failure(
        &self,
        service: &Arc<dyn ManagedService>,
        timestamp_ms: u64,
    ) -> SupervisorResult<()> {
        let name = service.descriptor().name();
        if service.runtime_state() == ServiceRuntimeState::Orphaned {
            return self.quarantine_orphan(
                &RestartCompletion {
                    service_id: name.to_string(),
                    attempt: self.restart_attempt(name),
                    outcome: RestartOutcome::Orphaned,
                },
                timestamp_ms,
            );
        }
        self.update_record(name, ServiceHealth::Failed, service.runtime_state())?;
        Ok(())
    }
}

fn service_snapshot(
    name: &str,
    service: &Arc<dyn ManagedService>,
    record: Option<&RuntimeRecord>,
) -> crate::ServiceSnapshot {
    let activation = service.descriptor().activation();
    let runtime_state = record
        .map(|record| record.runtime_state)
        .unwrap_or_else(|| service.runtime_state());
    crate::ServiceSnapshot {
        name: name.to_string(),
        health: record
            .and_then(|record| record.health)
            .unwrap_or_else(|| service.health()),
        dependencies: service
            .descriptor()
            .dependencies()
            .iter()
            .map(|dependency| dependency.service_id().to_string())
            .collect(),
        dependency_requirements: service
            .descriptor()
            .dependencies()
            .iter()
            .map(|dependency| format!("{:?}", dependency.requirement()))
            .collect(),
        activation,
        enabled: activation.is_enabled(),
        configured: activation.is_configured(),
        running: runtime_state == ServiceRuntimeState::Running,
        runtime_state,
        restart_state: record
            .map(|record| record.restart_state)
            .unwrap_or(RestartState::None),
        restart_count: record.map(|record| record.restart_count).unwrap_or(0),
        operator_required: record
            .map(|record| record.operator_required)
            .unwrap_or(false),
        quarantined: record.map(|record| record.quarantined).unwrap_or(false),
        critical: service.descriptor().is_critical(),
    }
}
