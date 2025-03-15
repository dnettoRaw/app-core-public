// =============================================================================
//        #######
//     ###       ###     F: job.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 10:59:21 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 11:51:10 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use crate::{ProviderError, ProviderResult};
use appcore_contracts::{CapabilityId, CoreId, JobId};

/// Provider-neutral description of one durable Runtime job.
#[derive(Clone, PartialEq, Eq)]
pub struct JobSpec {
    job_id: JobId,
    capability: CapabilityId,
    payload_reference: String,
    available_at_ms: u64,
    max_attempts: u32,
}

impl std::fmt::Debug for JobSpec {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("JobSpec")
            .field("job_id", &self.job_id)
            .field("capability", &self.capability)
            .field("payload_reference", &"REDACTED")
            .field("available_at_ms", &self.available_at_ms)
            .field("max_attempts", &self.max_attempts)
            .finish()
    }
}

impl JobSpec {
    /// Creates a job containing an opaque external payload reference.
    pub fn new(
        job_id: JobId,
        capability: CapabilityId,
        payload_reference: impl Into<String>,
        available_at_ms: u64,
        max_attempts: u32,
    ) -> ProviderResult<Self> {
        let payload_reference = payload_reference.into();
        validate_payload_reference(&payload_reference)?;
        if max_attempts == 0 {
            return Err(ProviderError::InvalidConfiguration(
                "job max_attempts must be greater than zero".to_string(),
            ));
        }
        Ok(Self {
            job_id,
            capability,
            payload_reference,
            available_at_ms,
            max_attempts,
        })
    }

    /// Returns the stable job identity.
    pub fn job_id(&self) -> &JobId {
        &self.job_id
    }

    /// Returns the capability required to execute the job.
    pub fn capability(&self) -> &CapabilityId {
        &self.capability
    }

    /// Returns the opaque provider-owned payload reference.
    pub fn payload_reference(&self) -> &str {
        &self.payload_reference
    }

    /// Returns the earliest execution timestamp.
    pub fn available_at_ms(&self) -> u64 {
        self.available_at_ms
    }

    /// Returns the bounded execution-attempt limit.
    pub fn max_attempts(&self) -> u32 {
        self.max_attempts
    }
}

/// Fenced lease returned when a Runtime core claims a job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobLease {
    job_id: JobId,
    holder_core_id: CoreId,
    epoch: u64,
    expires_at_ms: u64,
}

impl JobLease {
    /// Creates a fenced job lease.
    pub fn new(
        job_id: JobId,
        holder_core_id: CoreId,
        epoch: u64,
        expires_at_ms: u64,
    ) -> ProviderResult<Self> {
        if epoch == 0 || expires_at_ms == 0 {
            return Err(ProviderError::InvalidConfiguration(
                "job lease epoch and expiration must be greater than zero".to_string(),
            ));
        }
        Ok(Self {
            job_id,
            holder_core_id,
            epoch,
            expires_at_ms,
        })
    }

    /// Returns the leased job identity.
    pub fn job_id(&self) -> &JobId {
        &self.job_id
    }

    /// Returns the core holding the lease.
    pub fn holder_core_id(&self) -> &CoreId {
        &self.holder_core_id
    }

    /// Returns the monotonic fencing epoch.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Returns the lease expiration timestamp.
    pub fn expires_at_ms(&self) -> u64 {
        self.expires_at_ms
    }
}

/// Controlled terminal or retry outcome for a claimed job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobCompletion {
    /// The job completed and must not be claimed again.
    Completed,
    /// The job failed permanently without exposing application error details.
    Failed,
    /// The job may be claimed again at the supplied timestamp.
    RetryAt(u64),
}

/// Atomicity guarantee required from every durable job provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobAtomicity {
    /// Submission is idempotent and claim/completion use an atomic fenced CAS.
    FencedCompareAndSwap,
}

/// Provider contract for durable, capability-routed Runtime jobs.
///
/// Providers must offer at-least-once delivery. `submit` atomically inserts by
/// `job_id` or confirms the equivalent existing job. `claim` atomically selects
/// one eligible job, increments its fencing epoch, and stores its lease.
/// `complete` atomically compares the full lease fence and applies exactly one
/// terminal or retry transition. A stale or duplicate fence must be rejected.
pub trait JobProvider: Send + Sync {
    /// Reports the atomicity model implemented by this provider.
    fn atomicity(&self) -> JobAtomicity {
        JobAtomicity::FencedCompareAndSwap
    }

    /// Persists a new job idempotently by `job_id`.
    fn submit(&self, job: JobSpec) -> ProviderResult<()>;

    /// Claims the next eligible job for a capability and returns a fenced lease.
    fn claim(
        &self,
        capability: &CapabilityId,
        holder_core_id: &CoreId,
        now_ms: u64,
        lease_duration_ms: u64,
    ) -> ProviderResult<Option<JobLease>>;

    /// Completes or reschedules a job while enforcing the supplied lease fence.
    fn complete(&self, lease: &JobLease, completion: JobCompletion) -> ProviderResult<()>;
}

fn validate_payload_reference(reference: &str) -> ProviderResult<()> {
    if reference.trim().is_empty()
        || reference.len() > 2_048
        || reference.chars().any(char::is_control)
    {
        return Err(ProviderError::InvalidConfiguration(
            "job payload reference is invalid".to_string(),
        ));
    }
    Ok(())
}
