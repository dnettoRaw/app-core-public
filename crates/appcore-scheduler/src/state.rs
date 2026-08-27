// =============================================================================
//        #######
//     ###       ###     F: state.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0
// =============================================================================

//! Versioned bounded state-provider contract for opt-in durable scheduling.

use std::fmt;

/// Maximum durable records accepted by one provider operation.
pub const MAX_SCHEDULER_STATE_RECORDS: usize = 1_024;
/// Maximum bytes in a durable owner identity.
pub const MAX_SCHEDULER_OWNER_ID_BYTES: usize = 128;
/// Maximum claim lifetime accepted by the V1 contract.
pub const MAX_SCHEDULER_CLAIM_TTL_MS: u64 = 24 * 60 * 60 * 1_000;
/// Maximum clock-skew allowance accepted during claim takeover.
pub const MAX_SCHEDULER_CLOCK_SKEW_MS: u64 = 5 * 60 * 1_000;

/// Recovery behavior when a persisted execution instant is already past.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurableTaskMisfirePolicyV1 {
    /// Admit one execution and then resume the declared schedule.
    FireOnce,
    /// Skip missed work and resume at the first future schedule instant.
    Skip,
}

/// Controlled state-provider failure without paths, payloads or provider text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchedulerStateError {
    /// A bounded public input or persisted field is invalid.
    InvalidState(&'static str),
    /// The configured record capacity was reached.
    CapacityExceeded {
        /// Maximum records admitted by the operation.
        max_records: usize,
    },
    /// Existing state contradicts the registered task definition.
    Conflict(&'static str),
    /// A stale owner or fencing epoch attempted to mutate current state.
    Fenced,
    /// Provider persistence or locking failed; details are intentionally omitted.
    Unavailable,
    /// The stored version is unknown, removed or newer than this implementation.
    UpdateRequired,
}

impl fmt::Display for SchedulerStateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UpdateRequired => formatter.write_str("NO MORE SUPPORTED PLEASE UPDATE"),
            _ => write!(formatter, "{self:?}"),
        }
    }
}

impl std::error::Error for SchedulerStateError {}

/// Immutable durable task registration supplied when its callback is installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulerStateRegistrationV1 {
    /// Stable task identity.
    pub task_id: String,
    /// SHA-256 hex digest of the schedule, retry and priority definition.
    pub definition_hash: String,
    /// Initial next execution as Unix epoch milliseconds.
    pub initial_next_run_ms: u64,
    /// Explicit recovery behavior for missed schedule instants.
    pub misfire_policy: DurableTaskMisfirePolicyV1,
}

impl SchedulerStateRegistrationV1 {
    /// Validates bounded identity, digest and time fields.
    pub fn validate(&self) -> Result<(), SchedulerStateError> {
        validate_task_id(&self.task_id)?;
        validate_definition_hash(&self.definition_hash)
    }
}

/// Bounded atomic request to claim one eligible durable task.
#[derive(Clone, PartialEq, Eq)]
pub struct SchedulerStateClaimRequestV1 {
    /// Stable task identity.
    pub task_id: String,
    /// Stable identity of the scheduler instance requesting ownership.
    pub owner_id: String,
    /// Claim observation as Unix epoch milliseconds.
    pub now_ms: u64,
    /// Requested non-zero claim lifetime.
    pub claim_ttl_ms: u64,
    /// Conservative allowance before another owner may take over.
    pub max_clock_skew_ms: u64,
}

impl fmt::Debug for SchedulerStateClaimRequestV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SchedulerStateClaimRequestV1")
            .field("now_ms", &self.now_ms)
            .field("claim_ttl_ms", &self.claim_ttl_ms)
            .field("max_clock_skew_ms", &self.max_clock_skew_ms)
            .finish_non_exhaustive()
    }
}

/// Current execution claim for one durable task.
#[derive(Clone, PartialEq, Eq)]
pub struct SchedulerStateClaimV1 {
    /// Stable task identity.
    pub task_id: String,
    /// Provider-issued monotonic fencing epoch.
    pub fencing_epoch: u64,
    /// Claim expiry as Unix epoch milliseconds.
    pub lease_until_ms: u64,
    /// One-based attempt admitted by this claim.
    pub attempt: u32,
    pub(crate) owner_id: String,
}

impl SchedulerStateClaimV1 {
    /// Builds a validated provider-issued claim without exposing its owner in debug output.
    pub fn new(
        task_id: String,
        owner_id: String,
        fencing_epoch: u64,
        lease_until_ms: u64,
        attempt: u32,
    ) -> Result<Self, SchedulerStateError> {
        validate_task_id(&task_id)?;
        validate_owner_id(&owner_id)?;
        if fencing_epoch == 0 || attempt == 0 {
            return Err(SchedulerStateError::InvalidState("invalid claim state"));
        }
        Ok(Self {
            task_id,
            owner_id,
            fencing_epoch,
            lease_until_ms,
            attempt,
        })
    }

    /// Returns the owner identity used for exact provider comparisons.
    pub fn owner_id(&self) -> &str {
        &self.owner_id
    }
}

impl fmt::Debug for SchedulerStateClaimV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SchedulerStateClaimV1")
            .field("fencing_epoch", &self.fencing_epoch)
            .field("lease_until_ms", &self.lease_until_ms)
            .field("attempt", &self.attempt)
            .finish_non_exhaustive()
    }
}

/// Persisted state recovered when a durable callback is registered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulerStateRecordV1 {
    /// Stable task identity.
    pub task_id: String,
    /// SHA-256 hex digest of the registered schedule, retry and priority definition.
    pub definition_hash: String,
    /// Next eligible execution as Unix epoch milliseconds.
    pub next_run_ms: u64,
    /// Attempts already admitted in the current execution cycle.
    pub attempts: u32,
    /// Explicit missed-execution policy.
    pub misfire_policy: DurableTaskMisfirePolicyV1,
    /// Whether a terminal one-shot receipt has been committed.
    pub completed: bool,
    /// Last successfully committed fencing epoch, when any.
    pub last_receipt_epoch: Option<u64>,
    /// Current unexpired or expired claim, when an attempt was admitted.
    pub claim: Option<SchedulerStateClaimV1>,
    /// Highest fencing epoch ever issued for this task.
    pub fencing_epoch: u64,
}

impl SchedulerStateRecordV1 {
    /// Validates bounded fields and cross-field fencing invariants.
    pub fn validate(&self) -> Result<(), SchedulerStateError> {
        SchedulerStateRegistrationV1 {
            task_id: self.task_id.clone(),
            definition_hash: self.definition_hash.clone(),
            initial_next_run_ms: self.next_run_ms,
            misfire_policy: self.misfire_policy,
        }
        .validate()?;
        if self.completed
            && (self.claim.is_some() || self.attempts != 0 || self.last_receipt_epoch.is_none())
            || self.last_receipt_epoch == Some(0)
            || self
                .last_receipt_epoch
                .is_some_and(|epoch| epoch > self.fencing_epoch)
            || self.fencing_epoch == 0 && (self.attempts != 0 || self.last_receipt_epoch.is_some())
            || self.claim.as_ref().is_some_and(|claim| {
                claim.task_id != self.task_id
                    || claim.fencing_epoch > self.fencing_epoch
                    || claim.attempt != self.attempts
            })
        {
            return Err(SchedulerStateError::InvalidState("invalid task state"));
        }
        if let Some(claim) = &self.claim {
            Self::validate_claim(claim)?;
        }
        Ok(())
    }

    fn validate_claim(claim: &SchedulerStateClaimV1) -> Result<(), SchedulerStateError> {
        SchedulerStateClaimV1::new(
            claim.task_id.clone(),
            claim.owner_id.clone(),
            claim.fencing_epoch,
            claim.lease_until_ms,
            claim.attempt,
        )?;
        Ok(())
    }
}

/// Exact completion applied only by the owner and epoch holding the claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulerStateCompletionV1 {
    /// Claim returned by the provider for this execution.
    pub claim: SchedulerStateClaimV1,
    /// Completion observation as Unix epoch milliseconds.
    pub completed_at_ms: u64,
    /// Next execution instant, or `None` for a terminal one-shot receipt.
    pub next_run_ms: Option<u64>,
    /// Whether this execution cycle is settled and writes a receipt.
    pub settled: bool,
}

/// Payload-free bounded observations for a scheduler state provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchedulerStateStatsV1 {
    /// Total retained durable task records.
    pub records: usize,
    /// Records with a current claim, including expired claims awaiting takeover.
    pub claimed: usize,
    /// Records holding a terminal receipt.
    pub completed: usize,
}

/// Atomic durable-state boundary used only by explicitly durable tasks.
pub trait SchedulerStateProvider: Send + Sync {
    /// Creates a record or recovers the matching existing definition atomically.
    fn register(
        &self,
        registration: &SchedulerStateRegistrationV1,
        max_records: usize,
    ) -> Result<SchedulerStateRecordV1, SchedulerStateError>;

    /// Reads one current record for reconciliation without mutating it.
    fn record(&self, task_id: &str) -> Result<Option<SchedulerStateRecordV1>, SchedulerStateError>;

    /// Claims eligible work and returns a new monotonic fencing epoch.
    fn try_claim(
        &self,
        request: &SchedulerStateClaimRequestV1,
    ) -> Result<Option<SchedulerStateClaimV1>, SchedulerStateError>;

    /// Extends only the exact current owner/epoch claim.
    fn renew_claim(
        &self,
        claim: &SchedulerStateClaimV1,
        now_ms: u64,
        lease_until_ms: u64,
    ) -> Result<(), SchedulerStateError>;

    /// Commits a success receipt or retry state for the exact current claim.
    fn complete(
        &self,
        completion: &SchedulerStateCompletionV1,
    ) -> Result<SchedulerStateRecordV1, SchedulerStateError>;

    /// Returns payload-free bounded provider observations.
    fn stats(&self) -> Result<SchedulerStateStatsV1, SchedulerStateError>;
}

pub(crate) fn validate_provider_bounds(
    max_records: usize,
    owner_id: &str,
    claim_ttl_ms: u64,
    max_clock_skew_ms: u64,
) -> Result<(), SchedulerStateError> {
    if max_records == 0 || max_records > MAX_SCHEDULER_STATE_RECORDS {
        return Err(SchedulerStateError::InvalidState("invalid record limit"));
    }
    validate_owner_id(owner_id)?;
    if claim_ttl_ms == 0 || claim_ttl_ms > MAX_SCHEDULER_CLAIM_TTL_MS {
        return Err(SchedulerStateError::InvalidState("invalid claim ttl"));
    }
    if max_clock_skew_ms > MAX_SCHEDULER_CLOCK_SKEW_MS {
        return Err(SchedulerStateError::InvalidState("invalid clock skew"));
    }
    Ok(())
}

pub(crate) fn validate_task_id(task_id: &str) -> Result<(), SchedulerStateError> {
    if task_id.is_empty()
        || task_id.len() > super::MAX_TASK_ID_BYTES
        || !task_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(SchedulerStateError::InvalidState("invalid task id"));
    }
    Ok(())
}

pub(crate) fn validate_owner_id(owner_id: &str) -> Result<(), SchedulerStateError> {
    if owner_id.is_empty()
        || owner_id.len() > MAX_SCHEDULER_OWNER_ID_BYTES
        || !owner_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(SchedulerStateError::InvalidState("invalid owner id"));
    }
    Ok(())
}

fn validate_definition_hash(definition_hash: &str) -> Result<(), SchedulerStateError> {
    if definition_hash.len() != 64 || !definition_hash.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(SchedulerStateError::InvalidState("invalid definition hash"));
    }
    Ok(())
}
