// =============================================================================
//        #######
//     ###       ###     F: runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 13:24:05 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::*;
use crate::executor::{FixedExecutor, SubmitError};

/// Cancellation handle for a registered task.
#[derive(Clone)]
pub struct TaskHandle {
    id: String,
    cancelled: Arc<AtomicBool>,
    scheduler: Weak<SchedulerInner>,
}

impl TaskHandle {
    /// Returns the task identity.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Requests cooperative cancellation.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(scheduler) = self.scheduler.upgrade() {
            scheduler.wakeup.notify_all();
        }
    }

    /// Reports whether cancellation was requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// Bounded local scheduler with cooperative cancellation and clean shutdown.
pub struct Scheduler {
    inner: Arc<SchedulerInner>,
    coordinator: Mutex<Option<JoinHandle<()>>>,
}

struct SchedulerInner {
    config: SchedulerConfig,
    state: Mutex<SchedulerState>,
    wakeup: Condvar,
    shutdown: Arc<AtomicBool>,
    active: AtomicUsize,
    inflight: AtomicUsize,
    sequence: AtomicU64,
    jitter_state: AtomicU64,
    dispatch_limit: usize,
    executor: FixedExecutor,
}

#[derive(Default)]
struct SchedulerState {
    tasks: HashMap<String, TaskEntry>,
}

struct TaskEntry {
    schedule: ParsedSchedule,
    retry: RetryPolicy,
    priority: u8,
    callback: TaskCallback,
    cancelled: Arc<AtomicBool>,
    next_run: SystemTime,
    dispatched: bool,
    running: bool,
    attempts: u32,
    order: u64,
    last_error: Option<String>,
    trace: Option<TraceContext>,
}

enum ParsedSchedule {
    Once,
    Interval(Duration),
    Cron(Box<Schedule>),
}

impl Scheduler {
    /// Starts a scheduler coordinator with validated bounds.
    pub fn new(config: SchedulerConfig) -> Result<Self, SchedulerError> {
        config.validate()?;
        let worker_count = config.max_concurrent_tasks.min(config.max_tasks);
        let dispatch_limit = worker_count.saturating_mul(2).min(config.max_tasks).max(1);
        let executor = FixedExecutor::new(worker_count, dispatch_limit)
            .map_err(|_| SchedulerError::WorkerPanicked)?;
        let inner = Arc::new(SchedulerInner {
            config,
            state: Mutex::new(SchedulerState::default()),
            wakeup: Condvar::new(),
            shutdown: Arc::new(AtomicBool::new(false)),
            active: AtomicUsize::new(0),
            inflight: AtomicUsize::new(0),
            sequence: AtomicU64::new(1),
            jitter_state: AtomicU64::new(now_seed()),
            dispatch_limit,
            executor,
        });
        let coordinator_inner = Arc::clone(&inner);
        let coordinator = thread::Builder::new()
            .name("appcore-scheduler".to_string())
            .spawn(move || coordinator_loop(coordinator_inner))
            .map_err(|_| SchedulerError::WorkerPanicked)?;
        Ok(Self {
            inner,
            coordinator: Mutex::new(Some(coordinator)),
        })
    }

    /// Registers a task and callback.
    pub fn schedule(
        &self,
        task: ScheduledTask,
        callback: TaskCallback,
    ) -> Result<TaskHandle, SchedulerError> {
        if self.inner.shutdown.load(Ordering::Acquire) {
            return Err(SchedulerError::Shutdown);
        }
        task.validate()?;
        let (schedule, next_run) = parse_schedule(task.schedule)?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut state = self.inner.state.lock();
        if self.inner.shutdown.load(Ordering::Acquire) {
            return Err(SchedulerError::Shutdown);
        }
        if state.tasks.contains_key(&task.id) {
            return Err(SchedulerError::DuplicateTask(task.id));
        }
        if state.tasks.len() >= self.inner.config.max_tasks {
            return Err(SchedulerError::CapacityExceeded {
                max_tasks: self.inner.config.max_tasks,
            });
        }
        let id = task.id;
        state.tasks.insert(
            id.clone(),
            TaskEntry {
                schedule,
                retry: task.retry,
                priority: task.priority,
                callback,
                cancelled: Arc::clone(&cancelled),
                next_run,
                dispatched: false,
                running: false,
                attempts: 0,
                order: self.inner.sequence.fetch_add(1, Ordering::Relaxed),
                last_error: None,
                trace: task.trace,
            },
        );
        drop(state);
        self.inner.wakeup.notify_all();
        Ok(TaskHandle {
            id,
            cancelled,
            scheduler: Arc::downgrade(&self.inner),
        })
    }

    /// Requests cancellation of a task by identity.
    pub fn cancel(&self, task_id: &str) -> bool {
        let state = self.inner.state.lock();
        let Some(task) = state.tasks.get(task_id) else {
            return false;
        };
        task.cancelled.store(true, Ordering::Release);
        drop(state);
        self.inner.wakeup.notify_all();
        true
    }

    /// Returns current scheduler and task state.
    pub fn snapshot(&self) -> SchedulerSnapshot {
        let state = self.inner.state.lock();
        let mut tasks = state
            .tasks
            .iter()
            .map(|(id, task)| TaskSnapshot {
                id: id.clone(),
                priority: task.priority,
                running: task.running,
                attempts: task.attempts,
                next_run: task.next_run,
                last_error: task.last_error.clone(),
                trace: task.trace.clone(),
            })
            .collect::<Vec<_>>();
        tasks.sort_by(|left, right| left.id.cmp(&right.id));
        SchedulerSnapshot {
            shutdown: self.inner.shutdown.load(Ordering::Acquire),
            active_tasks: self.inner.active.load(Ordering::Acquire),
            tasks,
        }
    }

    /// Returns the fixed number of callback worker threads.
    pub fn worker_thread_count(&self) -> usize {
        self.inner.executor.worker_count()
    }

    /// Returns callbacks accepted by the bounded executor but not yet running.
    pub fn queued_task_count(&self) -> usize {
        self.inner.executor.queue_depth()
    }

    /// Returns how often due work exceeded the bounded dispatch capacity.
    pub fn queue_saturation_count(&self) -> u64 {
        self.inner.executor.saturation_count()
    }

    /// Stops accepting work, cancels tasks and joins all worker threads.
    pub fn shutdown(&self) -> Result<(), SchedulerError> {
        self.inner.shutdown.store(true, Ordering::Release);
        {
            let state = self.inner.state.lock();
            for task in state.tasks.values() {
                task.cancelled.store(true, Ordering::Release);
            }
        }
        self.inner.wakeup.notify_all();
        if let Some(coordinator) = self.coordinator.lock().take() {
            coordinator
                .join()
                .map_err(|_| SchedulerError::WorkerPanicked)?;
        }
        Ok(())
    }
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn coordinator_loop(inner: Arc<SchedulerInner>) {
    while !inner.shutdown.load(Ordering::Acquire) {
        let available = inner
            .dispatch_limit
            .saturating_sub(inner.inflight.load(Ordering::Acquire));
        let due = {
            let now = SystemTime::now();
            let mut state = inner.state.lock();
            state
                .tasks
                .retain(|_, task| task.dispatched || !task.cancelled.load(Ordering::Acquire));
            let mut due = state
                .tasks
                .iter()
                .filter(|(_, task)| !task.dispatched && task.next_run <= now)
                .map(|(id, task)| (id.clone(), task.priority, task.next_run, task.order))
                .collect::<Vec<_>>();
            due.sort_by(|left, right| {
                right
                    .1
                    .cmp(&left.1)
                    .then_with(|| left.2.cmp(&right.2))
                    .then_with(|| left.3.cmp(&right.3))
            });
            if due.len() > available {
                inner.executor.record_saturation();
            }
            due.truncate(available);
            for (id, _, _, _) in &due {
                if let Some(task) = state.tasks.get_mut(id) {
                    task.dispatched = true;
                    task.attempts = task.attempts.saturating_add(1);
                }
            }
            due.into_iter().map(|item| item.0).collect::<Vec<_>>()
        };

        for task_id in due {
            dispatch_task(&inner, task_id);
        }

        let mut state = inner.state.lock();
        if !inner.shutdown.load(Ordering::Acquire) {
            inner
                .wakeup
                .wait_for(&mut state, inner.config.poll_interval);
        }
    }

    inner.executor.shutdown();
    inner.state.lock().tasks.clear();
}

fn dispatch_task(inner: &Arc<SchedulerInner>, task_id: String) {
    let (callback, context) = {
        let state = inner.state.lock();
        let Some(task) = state.tasks.get(&task_id) else {
            return;
        };
        (
            Arc::clone(&task.callback),
            TaskContext::new(
                task_id.clone(),
                task.attempts,
                Arc::clone(&task.cancelled),
                Arc::clone(&inner.shutdown),
                task.trace.clone(),
            ),
        )
    };
    inner.inflight.fetch_add(1, Ordering::AcqRel);
    let worker_inner = Arc::clone(inner);
    let worker_task_id = task_id.clone();
    let job = Box::new(move || {
        mark_task_running(&worker_inner, &worker_task_id);
        worker_inner.active.fetch_add(1, Ordering::AcqRel);
        let result = catch_unwind(AssertUnwindSafe(|| callback(context)))
            .unwrap_or_else(|_| Err("task panicked".to_string()));
        worker_inner.active.fetch_sub(1, Ordering::AcqRel);
        complete_task(&worker_inner, &worker_task_id, result);
    });
    if let Err(error) = inner.executor.try_submit(job) {
        inner.inflight.fetch_sub(1, Ordering::AcqRel);
        defer_task(inner, &task_id, error);
    }
}

fn mark_task_running(inner: &SchedulerInner, task_id: &str) {
    if let Some(task) = inner.state.lock().tasks.get_mut(task_id) {
        task.running = true;
    }
}

fn defer_task(inner: &SchedulerInner, task_id: &str, error: SubmitError) {
    let mut state = inner.state.lock();
    let remove = inner.shutdown.load(Ordering::Acquire)
        || state
            .tasks
            .get(task_id)
            .is_some_and(|task| task.cancelled.load(Ordering::Acquire));
    if remove {
        state.tasks.remove(task_id);
    } else if let Some(task) = state.tasks.get_mut(task_id) {
        task.dispatched = false;
        task.attempts = task.attempts.saturating_sub(1);
        if matches!(error, SubmitError::Closed) {
            task.last_error = Some("scheduler executor unavailable".to_string());
        }
    }
    drop(state);
    inner.wakeup.notify_all();
}

fn complete_task(inner: &SchedulerInner, task_id: &str, result: TaskResult) {
    let now = SystemTime::now();
    let mut state = inner.state.lock();
    let mut remove = false;
    if let Some(task) = state.tasks.get_mut(task_id) {
        task.dispatched = false;
        task.running = false;
        if inner.shutdown.load(Ordering::Acquire) || task.cancelled.load(Ordering::Acquire) {
            remove = true;
        } else if let Err(error) = result {
            task.last_error = Some(redact_text(&error));
            if task.attempts < task.retry.max_attempts {
                if let Some(next_run) =
                    now.checked_add(retry_delay(&task.retry, task.attempts, inner))
                {
                    task.next_run = next_run;
                } else {
                    task.last_error = Some("retry schedule exceeds clock range".to_string());
                    remove = true;
                }
            } else {
                task.attempts = 0;
                remove = !schedule_next(&task.schedule, now, &mut task.next_run);
            }
        } else {
            task.last_error = None;
            task.attempts = 0;
            remove = !schedule_next(&task.schedule, now, &mut task.next_run);
        }
    }
    if remove {
        state.tasks.remove(task_id);
    }
    drop(state);
    inner.inflight.fetch_sub(1, Ordering::AcqRel);
    inner.wakeup.notify_all();
}

fn schedule_next(schedule: &ParsedSchedule, now: SystemTime, next_run: &mut SystemTime) -> bool {
    match schedule {
        ParsedSchedule::Once => false,
        ParsedSchedule::Interval(every) => {
            let Some(next) = now.checked_add(*every) else {
                return false;
            };
            *next_run = next;
            true
        }
        ParsedSchedule::Cron(schedule) => {
            let now: DateTime<Utc> = now.into();
            let Some(next) = schedule.after(&now).next() else {
                return false;
            };
            *next_run = next.into();
            true
        }
    }
}

fn parse_schedule(schedule: TaskSchedule) -> Result<(ParsedSchedule, SystemTime), SchedulerError> {
    let now = SystemTime::now();
    match schedule {
        TaskSchedule::Once { run_at } => Ok((ParsedSchedule::Once, run_at)),
        TaskSchedule::Interval { every, start_at } => {
            if every.is_zero() {
                return Err(SchedulerError::InvalidSchedule("zero interval"));
            }
            let next_run = match start_at {
                Some(start_at) => start_at,
                None => now
                    .checked_add(every)
                    .ok_or(SchedulerError::InvalidSchedule(
                        "interval exceeds clock range",
                    ))?,
            };
            Ok((ParsedSchedule::Interval(every), next_run))
        }
        TaskSchedule::Cron { expression } => {
            let schedule = Schedule::from_str(&expression)
                .map_err(|error| SchedulerError::InvalidCron(error.to_string()))?;
            let now_utc: DateTime<Utc> = now.into();
            let next = schedule
                .after(&now_utc)
                .next()
                .ok_or(SchedulerError::InvalidSchedule(
                    "cron has no next occurrence",
                ))?;
            Ok((ParsedSchedule::Cron(Box::new(schedule)), next.into()))
        }
    }
}

fn retry_delay(policy: &RetryPolicy, attempt: u32, inner: &SchedulerInner) -> Duration {
    let exponent = attempt.saturating_sub(1).min(31);
    let factor = u128::from(policy.multiplier).saturating_pow(exponent);
    let base_ms = policy.initial_backoff.as_millis().saturating_mul(factor);
    let capped_ms = base_ms.min(policy.max_backoff.as_millis());
    let jitter_max = policy.jitter.as_millis().min(u128::from(u64::MAX)) as u64;
    let jitter = if jitter_max == 0 {
        0
    } else {
        next_random(&inner.jitter_state) % jitter_max.saturating_add(1)
    };
    Duration::from_millis(
        capped_ms
            .min(u128::from(u64::MAX))
            .saturating_add(u128::from(jitter))
            .min(u128::from(u64::MAX)) as u64,
    )
}

fn next_random(state: &AtomicU64) -> u64 {
    let mut current = state.load(Ordering::Relaxed);
    loop {
        let mut next = current;
        next ^= next << 13;
        next ^= next >> 7;
        next ^= next << 17;
        match state.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return next,
            Err(actual) => current = actual,
        }
    }
}

fn now_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(1)
        .max(1)
}
