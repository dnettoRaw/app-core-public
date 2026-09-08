// =============================================================================
//        #######
//     ###       ###     F: runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 13:24:05 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Defines bounded runtime contracts and behavior for this crate.

use super::*;
use crate::durable::{
    durable_state, ms_to_system_time, system_time_to_ms, DurableRuntime, DurableTaskState,
};
use crate::executor::FixedExecutor;
use crate::runtime_loop::coordinator_loop;
use crate::timing::{now_seed, parse_schedule, ParsedSchedule};

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

pub(super) struct SchedulerInner {
    pub(super) config: SchedulerConfig,
    pub(super) durable: Option<DurableRuntime>,
    pub(super) state: Mutex<SchedulerState>,
    pub(super) wakeup: Condvar,
    pub(super) shutdown: Arc<AtomicBool>,
    pub(super) active: AtomicUsize,
    pub(super) inflight: AtomicUsize,
    pub(super) sequence: AtomicU64,
    pub(super) jitter_state: AtomicU64,
    pub(super) state_errors: AtomicU64,
    pub(super) dispatch_limit: usize,
    pub(super) executor: FixedExecutor,
}

#[derive(Default)]
pub(super) struct SchedulerState {
    pub(super) tasks: HashMap<String, TaskEntry>,
}

pub(super) struct TaskEntry {
    pub(super) schedule: ParsedSchedule,
    pub(super) retry: RetryPolicy,
    pub(super) priority: u8,
    pub(super) callback: TaskCallback,
    pub(super) cancelled: Arc<AtomicBool>,
    pub(super) next_run: SystemTime,
    pub(super) dispatched: bool,
    pub(super) running: bool,
    pub(super) attempts: u32,
    pub(super) order: u64,
    pub(super) last_error: Option<String>,
    pub(super) trace: Option<TraceContext>,
    pub(super) durable: Option<DurableTaskState>,
}

impl Scheduler {
    /// Starts a scheduler coordinator with validated bounds.
    pub fn new(config: SchedulerConfig) -> Result<Self, SchedulerError> {
        config.validate()?;
        Self::start(config, None)
    }

    /// Starts a scheduler that can explicitly register durable tasks.
    pub fn with_state_provider(
        config: SchedulerConfig,
        durable_config: DurableSchedulerConfigV1,
        provider: Arc<dyn SchedulerStateProvider>,
    ) -> Result<Self, SchedulerError> {
        config.validate()?;
        let durable = DurableRuntime::new(
            durable_config,
            provider,
            config.max_tasks,
            config.poll_interval,
        )?;
        Self::start(config, Some(durable))
    }

    fn start(
        config: SchedulerConfig,
        durable: Option<DurableRuntime>,
    ) -> Result<Self, SchedulerError> {
        let worker_count = config.max_concurrent_tasks.min(config.max_tasks);
        let dispatch_limit = worker_count.saturating_mul(2).min(config.max_tasks).max(1);
        let executor = FixedExecutor::new(worker_count, dispatch_limit)
            .map_err(|_| SchedulerError::WorkerPanicked)?;
        let inner = Arc::new(SchedulerInner {
            config,
            durable,
            state: Mutex::new(SchedulerState::default()),
            wakeup: Condvar::new(),
            shutdown: Arc::new(AtomicBool::new(false)),
            active: AtomicUsize::new(0),
            inflight: AtomicUsize::new(0),
            sequence: AtomicU64::new(1),
            jitter_state: AtomicU64::new(now_seed()),
            state_errors: AtomicU64::new(0),
            dispatch_limit,
            executor,
        });
        let coordinator_inner = Arc::clone(&inner);
        let coordinator = thread::Builder::new()
            .name("appcore-scheduler".to_string())
            .stack_size(crate::SCHEDULER_THREAD_STACK_BYTES)
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
        self.schedule_inner(task, callback, None)
    }

    /// Registers an explicitly durable task under the selected misfire policy.
    pub fn schedule_durable(
        &self,
        task: ScheduledTask,
        misfire_policy: DurableTaskMisfirePolicyV1,
        callback: TaskCallback,
    ) -> Result<TaskHandle, SchedulerError> {
        self.schedule_inner(task, callback, Some(misfire_policy))
    }

    fn schedule_inner(
        &self,
        task: ScheduledTask,
        callback: TaskCallback,
        misfire_policy: Option<DurableTaskMisfirePolicyV1>,
    ) -> Result<TaskHandle, SchedulerError> {
        if self.inner.shutdown.load(Ordering::Acquire) {
            return Err(SchedulerError::Shutdown);
        }
        task.validate()?;
        self.ensure_task_capacity(&task.id)?;
        let (schedule, initial_next_run) = parse_schedule(task.schedule.clone())?;
        let now_ms = system_time_to_ms(SystemTime::now())?;
        let (next_run, attempts, durable) = if let Some(misfire_policy) = misfire_policy {
            let runtime = self
                .inner
                .durable
                .as_ref()
                .ok_or(SchedulerError::InvalidConfig(
                    "state provider is not configured",
                ))?;
            let initial_next_run_ms = system_time_to_ms(initial_next_run)?;
            let record = runtime.register(&task, initial_next_run_ms, misfire_policy)?;
            if record.completed {
                return Ok(self.inactive_handle(task.id));
            }
            (
                ms_to_system_time(record.next_run_ms)?,
                record.attempts,
                Some(durable_state(&record, now_ms)),
            )
        } else {
            (initial_next_run, 0, None)
        };
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
                attempts,
                order: self.inner.sequence.fetch_add(1, Ordering::Relaxed),
                last_error: None,
                trace: task.trace,
                durable,
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

    fn ensure_task_capacity(&self, task_id: &str) -> Result<(), SchedulerError> {
        let state = self.inner.state.lock();
        if state.tasks.contains_key(task_id) {
            return Err(SchedulerError::DuplicateTask(task_id.to_string()));
        }
        if state.tasks.len() >= self.inner.config.max_tasks {
            return Err(SchedulerError::CapacityExceeded {
                max_tasks: self.inner.config.max_tasks,
            });
        }
        Ok(())
    }

    fn inactive_handle(&self, id: String) -> TaskHandle {
        TaskHandle {
            id,
            cancelled: Arc::new(AtomicBool::new(false)),
            scheduler: Arc::downgrade(&self.inner),
        }
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

    /// Returns durable provider failures observed after task registration.
    pub fn state_provider_error_count(&self) -> u64 {
        self.inner.state_errors.load(Ordering::Acquire)
    }

    /// Requests cancellation and waits up to five seconds for worker shutdown.
    /// Returns `SchedulerError::Shutdown` if callbacks have not finished. The
    /// scheduler remains closed; do not replace it while its workers are alive.
    pub fn shutdown(&self) -> Result<(), SchedulerError> {
        if self.shutdown_with_timeout(Duration::from_secs(5))? {
            Ok(())
        } else {
            Err(SchedulerError::Shutdown)
        }
    }

    /// Closes admission and uses `timeout` as the coordinator completion wait budget.
    /// `Ok(false)` means shutdown remains incomplete, not successful teardown.
    /// The coordinator and workers remain owned and cannot restart in this
    /// scheduler. Call again to observe completion. Dropping the scheduler
    /// detaches unfinished threads; their resources remain live until return.
    /// Callbacks and providers must cooperate; Rust cannot safely kill threads.
    pub fn shutdown_with_timeout(&self, timeout: Duration) -> Result<bool, SchedulerError> {
        let started = std::time::Instant::now();
        self.inner.shutdown.store(true, Ordering::Release);
        if let Some(state) = self.inner.state.try_lock() {
            for task in state.tasks.values() {
                task.cancelled.store(true, Ordering::Release);
            }
        }
        // TaskContext observes this shared flag without acquiring task/provider locks.
        self.inner.wakeup.notify_all();
        loop {
            if let Some(mut coordinator) = self.coordinator.try_lock() {
                if coordinator.as_ref().is_none_or(JoinHandle::is_finished) {
                    if let Some(handle) = coordinator.take() {
                        handle.join().map_err(|_| SchedulerError::WorkerPanicked)?;
                    }
                    return Ok(true);
                }
            }
            let Some(remaining) = timeout.checked_sub(started.elapsed()) else {
                return Ok(false);
            };
            if remaining.is_zero() {
                return Ok(false);
            }
            std::thread::sleep(remaining.min(Duration::from_millis(1)));
        }
    }
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        let _ = self.shutdown_with_timeout(Duration::ZERO);
    }
}
