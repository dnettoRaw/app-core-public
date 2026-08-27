// =============================================================================
//        #######
//     ###       ###     F: durable.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/27 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/27 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0
// =============================================================================

//! Opt-in durable scheduler runtime configuration and canonical definitions.

use crate::state::{validate_provider_bounds, MAX_SCHEDULER_STATE_RECORDS};
use crate::{
    DurableTaskMisfirePolicyV1, RetryPolicy, ScheduledTask, SchedulerError,
    SchedulerStateClaimRequestV1, SchedulerStateClaimV1, SchedulerStateCompletionV1,
    SchedulerStateError, SchedulerStateProvider, SchedulerStateRecordV1,
    SchedulerStateRegistrationV1, TaskSchedule,
};
use sha2::{Digest, Sha256};
use std::fmt;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Process identity and lease bounds for opt-in durable task execution.
#[derive(Clone, PartialEq, Eq)]
pub struct DurableSchedulerConfigV1 {
    owner_id: String,
    claim_ttl: Duration,
    max_clock_skew: Duration,
}

impl DurableSchedulerConfigV1 {
    /// Creates a validated durable scheduler configuration.
    pub fn new(
        owner_id: impl Into<String>,
        claim_ttl: Duration,
        max_clock_skew: Duration,
    ) -> Result<Self, SchedulerStateError> {
        let config = Self {
            owner_id: owner_id.into(),
            claim_ttl,
            max_clock_skew,
        };
        validate_provider_bounds(
            1,
            &config.owner_id,
            duration_ms(config.claim_ttl, "invalid claim ttl")?,
            duration_ms_allow_zero(config.max_clock_skew, "invalid clock skew")?,
        )?;
        Ok(config)
    }

    /// Returns the stable identity of this concurrently running scheduler.
    pub fn owner_id(&self) -> &str {
        &self.owner_id
    }

    /// Returns the configured execution-claim lifetime.
    pub fn claim_ttl(&self) -> Duration {
        self.claim_ttl
    }

    /// Returns the conservative takeover allowance.
    pub fn max_clock_skew(&self) -> Duration {
        self.max_clock_skew
    }
}

impl fmt::Debug for DurableSchedulerConfigV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DurableSchedulerConfigV1")
            .field("claim_ttl", &self.claim_ttl)
            .field("max_clock_skew", &self.max_clock_skew)
            .finish_non_exhaustive()
    }
}

pub(super) struct DurableRuntime {
    provider: Arc<dyn SchedulerStateProvider>,
    owner_id: String,
    pub(super) claim_ttl_ms: u64,
    max_clock_skew_ms: u64,
    max_records: usize,
}

pub(super) struct DurableTaskState {
    pub(super) misfire_pending: bool,
    pub(super) claim: Option<SchedulerStateClaimV1>,
    pub(super) lease_valid: Option<Arc<AtomicBool>>,
    pub(super) pending_completion: Option<SchedulerStateCompletionV1>,
    pub(super) provider_retry_at: SystemTime,
}

impl DurableRuntime {
    pub(super) fn new(
        config: DurableSchedulerConfigV1,
        provider: Arc<dyn SchedulerStateProvider>,
        max_tasks: usize,
        poll_interval: Duration,
    ) -> Result<Self, SchedulerError> {
        let claim_ttl_ms = duration_ms(config.claim_ttl, "invalid claim ttl")
            .map_err(SchedulerError::StateProvider)?;
        let max_clock_skew_ms = duration_ms_allow_zero(config.max_clock_skew, "invalid clock skew")
            .map_err(SchedulerError::StateProvider)?;
        let minimum_ttl = poll_interval
            .checked_mul(2)
            .ok_or(SchedulerError::InvalidConfig(
                "poll interval exceeds clock range",
            ))?;
        if config.claim_ttl < minimum_ttl {
            return Err(SchedulerError::InvalidConfig(
                "claim_ttl must cover two poll intervals",
            ));
        }
        Ok(Self {
            provider,
            owner_id: config.owner_id,
            claim_ttl_ms,
            max_clock_skew_ms,
            max_records: max_tasks.min(MAX_SCHEDULER_STATE_RECORDS),
        })
    }

    pub(super) fn register(
        &self,
        task: &ScheduledTask,
        initial_next_run_ms: u64,
        misfire_policy: DurableTaskMisfirePolicyV1,
    ) -> Result<SchedulerStateRecordV1, SchedulerError> {
        let registration = SchedulerStateRegistrationV1 {
            task_id: task.id.clone(),
            definition_hash: definition_hash(task)?,
            initial_next_run_ms,
            misfire_policy,
        };
        let record = self
            .provider
            .register(&registration, self.max_records)
            .map_err(SchedulerError::StateProvider)?;
        record.validate().map_err(SchedulerError::StateProvider)?;
        if record.task_id != registration.task_id
            || record.definition_hash != registration.definition_hash
            || record.misfire_policy != registration.misfire_policy
        {
            return Err(SchedulerError::StateProvider(
                SchedulerStateError::InvalidState("provider returned mismatched task state"),
            ));
        }
        Ok(record)
    }

    pub(super) fn record(
        &self,
        task_id: &str,
    ) -> Result<Option<SchedulerStateRecordV1>, SchedulerStateError> {
        let record = self.provider.record(task_id)?;
        if let Some(record) = &record {
            record.validate()?;
            if record.task_id != task_id {
                return Err(SchedulerStateError::InvalidState(
                    "provider returned mismatched task state",
                ));
            }
        }
        Ok(record)
    }

    pub(super) fn try_claim(
        &self,
        task_id: &str,
        now_ms: u64,
    ) -> Result<Option<SchedulerStateClaimV1>, SchedulerStateError> {
        self.provider.try_claim(&SchedulerStateClaimRequestV1 {
            task_id: task_id.to_string(),
            owner_id: self.owner_id.clone(),
            now_ms,
            claim_ttl_ms: self.claim_ttl_ms,
            max_clock_skew_ms: self.max_clock_skew_ms,
        })
    }

    pub(super) fn renew(
        &self,
        claim: &SchedulerStateClaimV1,
        now_ms: u64,
    ) -> Result<u64, SchedulerStateError> {
        let lease_until_ms = now_ms
            .checked_add(self.claim_ttl_ms)
            .ok_or(SchedulerStateError::InvalidState("claim time overflow"))?;
        self.provider.renew_claim(claim, now_ms, lease_until_ms)?;
        Ok(lease_until_ms)
    }

    pub(super) fn complete(
        &self,
        completion: &SchedulerStateCompletionV1,
    ) -> Result<SchedulerStateRecordV1, SchedulerStateError> {
        let record = self.provider.complete(completion)?;
        record.validate()?;
        if record.task_id != completion.claim.task_id {
            return Err(SchedulerStateError::InvalidState(
                "provider returned mismatched task state",
            ));
        }
        Ok(record)
    }
}

pub(super) fn durable_state(record: &SchedulerStateRecordV1, now_ms: u64) -> DurableTaskState {
    DurableTaskState {
        misfire_pending: record.misfire_policy == DurableTaskMisfirePolicyV1::Skip
            && record.next_run_ms <= now_ms,
        claim: None,
        lease_valid: None,
        pending_completion: None,
        provider_retry_at: SystemTime::now(),
    }
}

pub(super) fn system_time_to_ms(time: SystemTime) -> Result<u64, SchedulerError> {
    let nanos = time
        .duration_since(UNIX_EPOCH)
        .map_err(|_| SchedulerError::InvalidSchedule("time precedes unix epoch"))?
        .as_nanos();
    let rounded = nanos
        .checked_add(999_999)
        .ok_or(SchedulerError::InvalidSchedule("time exceeds clock range"))?
        / 1_000_000;
    u64::try_from(rounded).map_err(|_| SchedulerError::InvalidSchedule("time exceeds clock range"))
}

pub(super) fn ms_to_system_time(value: u64) -> Result<SystemTime, SchedulerError> {
    UNIX_EPOCH
        .checked_add(Duration::from_millis(value))
        .ok_or(SchedulerError::InvalidSchedule("time exceeds clock range"))
}

fn definition_hash(task: &ScheduledTask) -> Result<String, SchedulerError> {
    let mut hasher = Sha256::new();
    hash_schedule(&mut hasher, &task.schedule)?;
    hash_retry(&mut hasher, &task.retry);
    hash_field(&mut hasher, &[task.priority]);
    let digest = hasher.finalize();
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn hash_schedule(hasher: &mut Sha256, schedule: &TaskSchedule) -> Result<(), SchedulerError> {
    match schedule {
        TaskSchedule::Once { run_at } => {
            hash_field(hasher, b"once");
            hash_system_time(hasher, *run_at)?;
        }
        TaskSchedule::Interval { every, start_at } => {
            hash_field(hasher, b"interval");
            hash_field(hasher, &every.as_nanos().to_be_bytes());
            match start_at {
                Some(start_at) => hash_system_time(hasher, *start_at)?,
                None => hash_field(hasher, b"none"),
            }
        }
        TaskSchedule::Cron { expression } => {
            hash_field(hasher, b"cron");
            hash_field(hasher, expression.as_bytes());
        }
    }
    Ok(())
}

fn hash_system_time(hasher: &mut Sha256, time: SystemTime) -> Result<(), SchedulerError> {
    let nanos = time
        .duration_since(UNIX_EPOCH)
        .map_err(|_| SchedulerError::InvalidSchedule("time precedes unix epoch"))?
        .as_nanos();
    hash_field(hasher, &nanos.to_be_bytes());
    Ok(())
}

fn hash_retry(hasher: &mut Sha256, retry: &RetryPolicy) {
    hash_field(hasher, &retry.max_attempts.to_be_bytes());
    hash_field(hasher, &retry.initial_backoff.as_nanos().to_be_bytes());
    hash_field(hasher, &retry.max_backoff.as_nanos().to_be_bytes());
    hash_field(hasher, &retry.multiplier.to_be_bytes());
    hash_field(hasher, &retry.jitter.as_nanos().to_be_bytes());
}

fn hash_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn duration_ms(duration: Duration, error: &'static str) -> Result<u64, SchedulerStateError> {
    let milliseconds = duration.as_millis();
    if milliseconds == 0 {
        return Err(SchedulerStateError::InvalidState(error));
    }
    u64::try_from(milliseconds).map_err(|_| SchedulerStateError::InvalidState(error))
}

fn duration_ms_allow_zero(
    duration: Duration,
    error: &'static str,
) -> Result<u64, SchedulerStateError> {
    u64::try_from(duration.as_millis()).map_err(|_| SchedulerStateError::InvalidState(error))
}
