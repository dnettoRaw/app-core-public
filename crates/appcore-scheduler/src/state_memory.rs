// =============================================================================
//        #######
//     ###       ###     F: state_memory.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0
// =============================================================================

//! Process-local reference implementation of the durable-state contract.

use crate::state::{
    validate_provider_bounds, validate_task_id, SchedulerStateClaimRequestV1,
    SchedulerStateClaimV1, SchedulerStateCompletionV1, SchedulerStateError, SchedulerStateProvider,
    SchedulerStateRecordV1, SchedulerStateRegistrationV1, SchedulerStateStatsV1,
};
use parking_lot::Mutex;
use std::collections::BTreeMap;

/// Process-local provider for conformance, tests and explicit ephemeral use.
#[derive(Debug, Default)]
pub struct InMemorySchedulerStateProvider {
    records: Mutex<BTreeMap<String, SchedulerStateRecordV1>>,
}

impl InMemorySchedulerStateProvider {
    /// Creates an empty state provider.
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn from_records(records: Vec<SchedulerStateRecordV1>) -> Self {
        Self {
            records: Mutex::new(
                records
                    .into_iter()
                    .map(|record| (record.task_id.clone(), record))
                    .collect(),
            ),
        }
    }

    pub(crate) fn records(&self) -> Vec<SchedulerStateRecordV1> {
        self.records.lock().values().cloned().collect()
    }
}

impl SchedulerStateProvider for InMemorySchedulerStateProvider {
    fn register(
        &self,
        registration: &SchedulerStateRegistrationV1,
        max_records: usize,
    ) -> Result<SchedulerStateRecordV1, SchedulerStateError> {
        registration.validate()?;
        if max_records == 0 || max_records > crate::state::MAX_SCHEDULER_STATE_RECORDS {
            return Err(SchedulerStateError::InvalidState("invalid record limit"));
        }
        let mut records = self.records.lock();
        if let Some(record) = records.get(&registration.task_id) {
            if record.definition_hash != registration.definition_hash
                || record.misfire_policy != registration.misfire_policy
            {
                return Err(SchedulerStateError::Conflict(
                    "durable task definition changed",
                ));
            }
            return Ok(record.clone());
        }
        if records.len() >= max_records {
            return Err(SchedulerStateError::CapacityExceeded { max_records });
        }
        let record = SchedulerStateRecordV1 {
            task_id: registration.task_id.clone(),
            definition_hash: registration.definition_hash.clone(),
            next_run_ms: registration.initial_next_run_ms,
            attempts: 0,
            misfire_policy: registration.misfire_policy,
            completed: false,
            last_receipt_epoch: None,
            claim: None,
            fencing_epoch: 0,
        };
        records.insert(registration.task_id.clone(), record.clone());
        Ok(record)
    }

    fn record(&self, task_id: &str) -> Result<Option<SchedulerStateRecordV1>, SchedulerStateError> {
        validate_task_id(task_id)?;
        Ok(self.records.lock().get(task_id).cloned())
    }

    fn try_claim(
        &self,
        request: &SchedulerStateClaimRequestV1,
    ) -> Result<Option<SchedulerStateClaimV1>, SchedulerStateError> {
        validate_provider_bounds(
            1,
            &request.owner_id,
            request.claim_ttl_ms,
            request.max_clock_skew_ms,
        )?;
        validate_task_id(&request.task_id)?;
        let mut records = self.records.lock();
        let record = records
            .get_mut(&request.task_id)
            .ok_or(SchedulerStateError::Conflict("durable task not registered"))?;
        if record.completed || record.next_run_ms > request.now_ms {
            return Ok(None);
        }
        if record.claim.as_ref().is_some_and(|claim| {
            claim
                .lease_until_ms
                .saturating_add(request.max_clock_skew_ms)
                >= request.now_ms
        }) {
            return Ok(None);
        }
        let fencing_epoch = record
            .fencing_epoch
            .checked_add(1)
            .ok_or(SchedulerStateError::InvalidState("fencing epoch overflow"))?;
        let attempt = record
            .attempts
            .checked_add(1)
            .ok_or(SchedulerStateError::InvalidState("attempt overflow"))?;
        let lease_until_ms = request
            .now_ms
            .checked_add(request.claim_ttl_ms)
            .ok_or(SchedulerStateError::InvalidState("claim time overflow"))?;
        let claim = SchedulerStateClaimV1::new(
            request.task_id.clone(),
            request.owner_id.clone(),
            fencing_epoch,
            lease_until_ms,
            attempt,
        )?;
        record.attempts = attempt;
        record.fencing_epoch = fencing_epoch;
        record.claim = Some(claim.clone());
        Ok(Some(claim))
    }

    fn renew_claim(
        &self,
        claim: &SchedulerStateClaimV1,
        now_ms: u64,
        lease_until_ms: u64,
    ) -> Result<(), SchedulerStateError> {
        let mut records = self.records.lock();
        let record = exact_claim(&mut records, claim)?;
        let current_until = record
            .claim
            .as_ref()
            .map_or(0, |current| current.lease_until_ms);
        if now_ms > current_until || lease_until_ms < current_until || lease_until_ms < now_ms {
            return Err(SchedulerStateError::InvalidState("claim time regression"));
        }
        if let Some(current) = record.claim.as_mut() {
            current.lease_until_ms = lease_until_ms;
        }
        Ok(())
    }

    fn complete(
        &self,
        completion: &SchedulerStateCompletionV1,
    ) -> Result<SchedulerStateRecordV1, SchedulerStateError> {
        if !completion.settled && completion.next_run_ms.is_none() {
            return Err(SchedulerStateError::InvalidState(
                "failed completion requires retry time",
            ));
        }
        let mut records = self.records.lock();
        let record = exact_claim(&mut records, &completion.claim)?;
        if completion.completed_at_ms
            > record
                .claim
                .as_ref()
                .map_or(0, |claim| claim.lease_until_ms)
        {
            return Err(SchedulerStateError::Fenced);
        }
        record.claim = None;
        if completion.settled {
            record.attempts = 0;
            record.last_receipt_epoch = Some(completion.claim.fencing_epoch);
            if let Some(next_run_ms) = completion.next_run_ms {
                record.next_run_ms = next_run_ms;
            } else {
                record.completed = true;
            }
        } else if let Some(next_run_ms) = completion.next_run_ms {
            record.next_run_ms = next_run_ms;
        }
        Ok(record.clone())
    }

    fn stats(&self) -> Result<SchedulerStateStatsV1, SchedulerStateError> {
        let records = self.records.lock();
        Ok(SchedulerStateStatsV1 {
            records: records.len(),
            claimed: records
                .values()
                .filter(|record| record.claim.is_some())
                .count(),
            completed: records.values().filter(|record| record.completed).count(),
        })
    }
}

fn exact_claim<'a>(
    records: &'a mut BTreeMap<String, SchedulerStateRecordV1>,
    claim: &SchedulerStateClaimV1,
) -> Result<&'a mut SchedulerStateRecordV1, SchedulerStateError> {
    let record = records
        .get_mut(&claim.task_id)
        .ok_or(SchedulerStateError::Fenced)?;
    if record.claim.as_ref().is_none_or(|current| {
        current.owner_id != claim.owner_id || current.fencing_epoch != claim.fencing_epoch
    }) {
        return Err(SchedulerStateError::Fenced);
    }
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DurableTaskMisfirePolicyV1;

    fn registration(task_id: &str) -> SchedulerStateRegistrationV1 {
        SchedulerStateRegistrationV1 {
            task_id: task_id.to_string(),
            definition_hash: "a".repeat(64),
            initial_next_run_ms: 100,
            misfire_policy: DurableTaskMisfirePolicyV1::FireOnce,
        }
    }

    fn claim_request(
        owner_id: &str,
        now_ms: u64,
        claim_ttl_ms: u64,
        max_clock_skew_ms: u64,
    ) -> SchedulerStateClaimRequestV1 {
        SchedulerStateClaimRequestV1 {
            task_id: "task-a".to_string(),
            owner_id: owner_id.to_string(),
            now_ms,
            claim_ttl_ms,
            max_clock_skew_ms,
        }
    }

    #[test]
    fn registration_recovers_exact_definition_and_bounds_capacity() {
        let provider = InMemorySchedulerStateProvider::new();
        let first = provider.register(&registration("task-a"), 1).unwrap();
        assert_eq!(first.next_run_ms, 100);
        assert_eq!(provider.register(&registration("task-a"), 1), Ok(first));
        assert!(matches!(
            provider.register(&registration("task-b"), 1),
            Err(SchedulerStateError::CapacityExceeded { max_records: 1 })
        ));

        let mut changed = registration("task-a");
        changed.definition_hash = "b".repeat(64);
        assert_eq!(
            provider.register(&changed, 1),
            Err(SchedulerStateError::Conflict(
                "durable task definition changed"
            ))
        );
    }

    #[test]
    fn claims_fence_two_owners_and_terminal_receipt_survives_registration() {
        let provider = InMemorySchedulerStateProvider::new();
        provider.register(&registration("task-a"), 4).unwrap();
        let first = provider
            .try_claim(&claim_request("owner-a", 100, 10, 5))
            .unwrap()
            .unwrap();
        assert_eq!(first.fencing_epoch, 1);
        assert_eq!(first.attempt, 1);
        assert_eq!(
            provider.try_claim(&claim_request("owner-b", 115, 10, 5)),
            Ok(None)
        );
        let second = provider
            .try_claim(&claim_request("owner-b", 116, 10, 5))
            .unwrap()
            .unwrap();
        assert_eq!(second.fencing_epoch, 2);
        assert_eq!(second.attempt, 2);
        assert_eq!(
            provider.complete(&SchedulerStateCompletionV1 {
                claim: first,
                completed_at_ms: 120,
                next_run_ms: None,
                settled: true,
            }),
            Err(SchedulerStateError::Fenced)
        );
        let completed = provider
            .complete(&SchedulerStateCompletionV1 {
                claim: second,
                completed_at_ms: 120,
                next_run_ms: None,
                settled: true,
            })
            .unwrap();
        assert!(completed.completed);
        assert_eq!(completed.last_receipt_epoch, Some(2));
        assert_eq!(
            provider.try_claim(&claim_request("owner-a", 1_000, 10, 0)),
            Ok(None)
        );
        assert!(
            provider
                .register(&registration("task-a"), 4)
                .unwrap()
                .completed
        );
    }

    #[test]
    fn retry_state_and_exact_renewal_are_atomic() {
        let provider = InMemorySchedulerStateProvider::new();
        provider.register(&registration("task-a"), 4).unwrap();
        let claim = provider
            .try_claim(&claim_request("owner-a", 100, 10, 0))
            .unwrap()
            .unwrap();
        assert_eq!(provider.renew_claim(&claim, 105, 120), Ok(()));
        let retried = provider
            .complete(&SchedulerStateCompletionV1 {
                claim,
                completed_at_ms: 119,
                next_run_ms: Some(150),
                settled: false,
            })
            .unwrap();
        assert_eq!(retried.attempts, 1);
        assert_eq!(retried.next_run_ms, 150);
        assert!(retried.claim.is_none());
        assert_eq!(
            provider.stats().unwrap(),
            SchedulerStateStatsV1 {
                records: 1,
                claimed: 0,
                completed: 0,
            }
        );
    }

    #[test]
    fn invalid_bounds_fail_without_echoing_dynamic_input() {
        let provider = InMemorySchedulerStateProvider::new();
        assert_eq!(
            provider.register(&registration("task-a"), 0),
            Err(SchedulerStateError::InvalidState("invalid record limit"))
        );
        assert_eq!(
            provider.try_claim(&claim_request("owner/a", 100, 10, 0)),
            Err(SchedulerStateError::InvalidState("invalid owner id"))
        );
    }
}
