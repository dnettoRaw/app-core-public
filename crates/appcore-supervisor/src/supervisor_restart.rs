// =============================================================================
//        #######
//     ###       ###     F: supervisor_restart.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/24 13:18:47 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 13:18:47 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Restart scheduling, bounded execution, budgets, and quarantine.

use super::*;

impl Supervisor {
    pub(super) fn schedule_restart(
        &self,
        service: &Arc<dyn ManagedService>,
        timestamp_ms: u64,
    ) -> SupervisorResult<()> {
        let name = service.descriptor().name();
        let policy = service.descriptor().restart_policy();
        if policy.mode == RestartMode::Never || self.restart_is_active(name)? {
            return Ok(());
        }
        let attempt = self.consume_restart_budget(service, timestamp_ms)?;
        let delay = policy.backoff.saturating_add(self.jitter(policy.jitter));
        let execute_at_ms = timestamp_ms.saturating_add(duration_ms(delay));
        {
            let mut records = self.records()?;
            let record = record_mut(&mut records, name)?;
            record.restart_state = RestartState::Scheduled { execute_at_ms };
            record.runtime_state = ServiceRuntimeState::RestartScheduled;
        }
        self.emit(
            name,
            SupervisorEventKind::ServiceRestartScheduled,
            timestamp_ms,
            attempt,
            ("Failed", "RestartScheduled"),
            "restart_policy",
        );
        Ok(())
    }

    pub(super) fn dispatch_due_restarts(&self, timestamp_ms: u64) -> SupervisorResult<()> {
        let due = {
            let records = self.records()?;
            records
                .iter()
                .filter_map(|(name, record)| match record.restart_state {
                    RestartState::Scheduled { execute_at_ms } if execute_at_ms <= timestamp_ms => {
                        Some((name.clone(), record.restart_count))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        for (name, attempt) in due {
            let service = self.service(&name)?;
            match self
                .inner
                .restart_executor
                .schedule(RestartCommand { service, attempt })
            {
                Ok(()) => {
                    let mut records = self.records()?;
                    let record = record_mut(&mut records, &name)?;
                    record.restart_state = RestartState::Stopping;
                    record.runtime_state = ServiceRuntimeState::Restarting;
                }
                Err(SupervisorError::RestartQueueFull) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    pub(super) fn apply_restart_completions(&self, timestamp_ms: u64) -> SupervisorResult<()> {
        for completion in self.inner.restart_executor.drain_completions() {
            self.apply_restart_completion(completion, timestamp_ms)?;
        }
        Ok(())
    }

    fn apply_restart_completion(
        &self,
        completion: RestartCompletion,
        timestamp_ms: u64,
    ) -> SupervisorResult<()> {
        let service = self.service(&completion.service_id)?;
        match completion.outcome {
            RestartOutcome::Restarted => {
                let health = service.health();
                {
                    let mut records = self.records()?;
                    let record = record_mut(&mut records, &completion.service_id)?;
                    record.restart_state = RestartState::None;
                    record.runtime_state = service.runtime_state();
                    record.health = Some(health);
                }
                self.emit(
                    &completion.service_id,
                    SupervisorEventKind::ServiceRestarted,
                    timestamp_ms,
                    completion.attempt,
                    ("Restarting", health_name(health)),
                    "restart_completed",
                );
            }
            RestartOutcome::Orphaned => self.quarantine_orphan(&completion, timestamp_ms)?,
            RestartOutcome::Failed | RestartOutcome::Cancelled => {
                let mut records = self.records()?;
                let record = record_mut(&mut records, &completion.service_id)?;
                record.restart_state = RestartState::Failed;
                record.runtime_state = service.runtime_state();
                record.health = Some(ServiceHealth::Failed);
                drop(records);
                self.emit(
                    &completion.service_id,
                    SupervisorEventKind::ServiceFailed,
                    timestamp_ms,
                    completion.attempt,
                    ("Restarting", "Failed"),
                    "restart_failed",
                );
            }
        }
        Ok(())
    }

    pub(super) fn quarantine_orphan(
        &self,
        completion: &RestartCompletion,
        timestamp_ms: u64,
    ) -> SupervisorResult<()> {
        {
            let mut records = self.records()?;
            let record = record_mut(&mut records, &completion.service_id)?;
            record.restart_state = RestartState::Failed;
            record.runtime_state = ServiceRuntimeState::Orphaned;
            record.health = Some(ServiceHealth::Failed);
            record.operator_required = true;
            record.quarantined = true;
        }
        self.emit(
            &completion.service_id,
            SupervisorEventKind::ServiceOrphaned,
            timestamp_ms,
            completion.attempt,
            ("Stopping", "Orphaned"),
            "shutdown_timeout",
        );
        self.emit(
            &completion.service_id,
            SupervisorEventKind::ServiceQuarantined,
            timestamp_ms,
            completion.attempt,
            ("Orphaned", "Quarantined"),
            "orphaned_instance",
        );
        Ok(())
    }

    fn consume_restart_budget(
        &self,
        service: &Arc<dyn ManagedService>,
        timestamp_ms: u64,
    ) -> SupervisorResult<u64> {
        let name = service.descriptor().name();
        let policy = service.descriptor().restart_policy();
        let cutoff = timestamp_ms.saturating_sub(duration_ms(policy.restart_window));
        let mut records = self.records()?;
        let record = record_mut(&mut records, name)?;
        record.restart_times_ms.retain(|attempt| *attempt > cutoff);
        if record.restart_times_ms.len() >= policy.restart_budget as usize {
            record.health = Some(ServiceHealth::Failed);
            record.runtime_state = ServiceRuntimeState::Quarantined;
            record.operator_required = true;
            record.quarantined = true;
            let attempt = record.restart_count;
            drop(records);
            self.emit(
                name,
                SupervisorEventKind::RestartBudgetExceeded,
                timestamp_ms,
                attempt,
                ("Failed", "Quarantined"),
                "restart_budget_exceeded",
            );
            self.emit(
                name,
                SupervisorEventKind::ServiceQuarantined,
                timestamp_ms,
                attempt,
                ("Failed", "Quarantined"),
                "operator_required",
            );
            return Err(SupervisorError::RestartBudgetExceeded(name.to_string()));
        }
        record.restart_times_ms.push_back(timestamp_ms);
        record.restart_count = record.restart_count.saturating_add(1);
        Ok(record.restart_count)
    }

    fn restart_is_active(&self, name: &str) -> SupervisorResult<bool> {
        let records = self.records()?;
        let record = records
            .get(name)
            .ok_or_else(|| SupervisorError::ServiceNotFound(name.to_string()))?;
        Ok(record.quarantined
            || matches!(
                record.restart_state,
                RestartState::Scheduled { .. }
                    | RestartState::Stopping
                    | RestartState::Starting
                    | RestartState::Backoff
            ))
    }
}
