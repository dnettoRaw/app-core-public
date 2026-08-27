// =============================================================================
//        #######
//     ###       ###     F: task.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::*;

/// Result returned by one scheduled task invocation.
pub type TaskResult = Result<(), String>;
/// Thread-safe scheduled task callback.
pub type TaskCallback = Arc<dyn Fn(TaskContext) -> TaskResult + Send + Sync + 'static>;

/// Controlled scheduler failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchedulerError {
    /// Scheduler configuration is invalid.
    InvalidConfig(&'static str),
    /// Task identity is empty, too long or malformed.
    InvalidTaskId,
    /// Task schedule or retry policy is invalid.
    InvalidSchedule(&'static str),
    /// Cron expression could not be parsed.
    InvalidCron(String),
    /// A task with the same identity already exists.
    DuplicateTask(String),
    /// Configured task capacity was reached.
    CapacityExceeded {
        /// Maximum registered tasks.
        max_tasks: usize,
    },
    /// Scheduler no longer accepts work after shutdown.
    Shutdown,
    /// The explicitly configured durable state provider rejected an operation.
    StateProvider(SchedulerStateError),
    /// Coordinator or worker thread panicked.
    WorkerPanicked,
}

impl fmt::Display for SchedulerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StateProvider(error) => error.fmt(formatter),
            _ => write!(formatter, "{self:?}"),
        }
    }
}

impl std::error::Error for SchedulerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::StateProvider(error) => Some(error),
            _ => None,
        }
    }
}

impl From<SchedulerStateError> for SchedulerError {
    fn from(error: SchedulerStateError) -> Self {
        Self::StateProvider(error)
    }
}

/// Bounded scheduler process configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulerConfig {
    /// Maximum registered tasks.
    pub max_tasks: usize,
    /// Maximum callbacks executing concurrently and fixed worker-pool size.
    pub max_concurrent_tasks: usize,
    /// Maximum coordinator sleep between due-task scans.
    pub poll_interval: Duration,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_tasks: 1_024,
            max_concurrent_tasks: 4,
            poll_interval: Duration::from_millis(25),
        }
    }
}

impl SchedulerConfig {
    pub(crate) fn validate(&self) -> Result<(), SchedulerError> {
        if self.max_tasks == 0 {
            return Err(SchedulerError::InvalidConfig("max_tasks must be positive"));
        }
        if self.max_concurrent_tasks == 0 {
            return Err(SchedulerError::InvalidConfig(
                "max_concurrent_tasks must be positive",
            ));
        }
        if self.poll_interval.is_zero() {
            return Err(SchedulerError::InvalidConfig(
                "poll_interval must be positive",
            ));
        }
        Ok(())
    }
}

/// Supported local task schedule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskSchedule {
    /// Run once at the supplied wall-clock instant.
    Once {
        /// Scheduled execution instant.
        run_at: SystemTime,
    },
    /// Run repeatedly at a fixed interval.
    Interval {
        /// Positive interval between executions.
        every: Duration,
        /// Optional first execution instant.
        start_at: Option<SystemTime>,
    },
    /// A six- or seven-field cron expression evaluated in UTC.
    Cron {
        /// UTC cron expression.
        expression: String,
    },
}

/// Bounded exponential retry policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Total attempts, including the initial execution.
    pub max_attempts: u32,
    /// Delay before the first retry.
    pub initial_backoff: Duration,
    /// Maximum delay between retries.
    pub max_backoff: Duration,
    /// Backoff multiplier.
    pub multiplier: u32,
    /// Maximum deterministic jitter window.
    pub jitter: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 1,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(30),
            multiplier: 2,
            jitter: Duration::ZERO,
        }
    }
}

/// Immutable scheduled task definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledTask {
    /// Stable task identity.
    pub id: String,
    /// Execution schedule.
    pub schedule: TaskSchedule,
    /// Retry policy.
    pub retry: RetryPolicy,
    /// Higher values run first when multiple tasks are due at the same time.
    pub priority: u8,
    /// Optional trace context propagated to callbacks.
    pub trace: Option<TraceContext>,
}

impl ScheduledTask {
    /// Validates the task definition without starting scheduler infrastructure.
    pub fn validate(&self) -> Result<(), SchedulerError> {
        validate_task_id(&self.id)?;
        validate_retry(&self.retry)?;
        validate_schedule(&self.schedule)
    }
}

fn validate_schedule(schedule: &TaskSchedule) -> Result<(), SchedulerError> {
    match schedule {
        TaskSchedule::Once { .. } => Ok(()),
        TaskSchedule::Interval { every, .. } if every.is_zero() => {
            Err(SchedulerError::InvalidSchedule("zero interval"))
        }
        TaskSchedule::Interval { .. } => Ok(()),
        TaskSchedule::Cron { expression } => Schedule::from_str(expression)
            .map(|_| ())
            .map_err(|error| SchedulerError::InvalidCron(error.to_string())),
    }
}

fn validate_retry(policy: &RetryPolicy) -> Result<(), SchedulerError> {
    if policy.max_attempts == 0 {
        return Err(SchedulerError::InvalidSchedule(
            "max_attempts must be positive",
        ));
    }
    if policy.multiplier == 0 {
        return Err(SchedulerError::InvalidSchedule(
            "retry multiplier must be positive",
        ));
    }
    if policy.initial_backoff > policy.max_backoff {
        return Err(SchedulerError::InvalidSchedule(
            "initial_backoff exceeds max_backoff",
        ));
    }
    Ok(())
}

fn validate_task_id(task_id: &str) -> Result<(), SchedulerError> {
    if task_id.is_empty()
        || task_id.len() > MAX_TASK_ID_BYTES
        || !task_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(SchedulerError::InvalidTaskId);
    }
    Ok(())
}

/// Observable state of one registered task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSnapshot {
    /// Stable task identity.
    pub id: String,
    /// Scheduling priority.
    pub priority: u8,
    /// Whether a callback is currently executing.
    pub running: bool,
    /// Attempts made for the current execution cycle.
    pub attempts: u32,
    /// Next planned wall-clock execution.
    pub next_run: SystemTime,
    /// Last redacted callback error.
    pub last_error: Option<String>,
    /// Optional trace context.
    pub trace: Option<TraceContext>,
}

/// Observable scheduler state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulerSnapshot {
    /// Whether shutdown was requested.
    pub shutdown: bool,
    /// Number of callbacks currently executing.
    pub active_tasks: usize,
    /// Registered task snapshots ordered by ID.
    pub tasks: Vec<TaskSnapshot>,
}
/// Cooperative execution context supplied to a task callback.
pub struct TaskContext {
    task_id: String,
    attempt: u32,
    cancelled: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    trace: Option<TraceContext>,
    fencing_epoch: Option<u64>,
    lease_valid: Option<Arc<AtomicBool>>,
}

impl TaskContext {
    pub(crate) fn new(
        task_id: String,
        attempt: u32,
        cancelled: Arc<AtomicBool>,
        shutdown: Arc<AtomicBool>,
        trace: Option<TraceContext>,
        fencing_epoch: Option<u64>,
        lease_valid: Option<Arc<AtomicBool>>,
    ) -> Self {
        Self {
            task_id,
            attempt,
            cancelled,
            shutdown,
            trace,
            fencing_epoch,
            lease_valid,
        }
    }

    /// Returns the stable task identity.
    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    /// Returns the current one-based attempt.
    pub fn attempt(&self) -> u32 {
        self.attempt
    }

    /// Reports whether task or scheduler cancellation was requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
            || self.shutdown.load(Ordering::Acquire)
            || self
                .lease_valid
                .as_ref()
                .is_some_and(|valid| !valid.load(Ordering::Acquire))
    }

    /// Returns propagated trace context.
    pub fn trace(&self) -> Option<&TraceContext> {
        self.trace.as_ref()
    }

    /// Returns the durable fencing epoch, or `None` for an ephemeral task.
    pub fn fencing_epoch(&self) -> Option<u64> {
        self.fencing_epoch
    }
}
