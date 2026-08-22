// =============================================================================
//        #######
//     ###       ###     F: execution_queue.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{
    AiError, AiPriority, AiResult, CancellationToken, FairQueue, FairQueueConfig, QueueAdmission,
};
use std::collections::BTreeSet;
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Runtime concurrency and fair-waiting bounds applied around backend routing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionQueueConfig {
    /// Maximum requests concurrently executing model routes.
    pub max_active: usize,
    /// Priority, starvation and overload policy for waiting requests.
    pub waiting: FairQueueConfig,
    /// Maximum condition-variable wait before cancellation/deadline is rechecked.
    pub cancellation_poll: Duration,
}

impl ExecutionQueueConfig {
    fn validate(self) -> AiResult<Self> {
        self.waiting.validate()?;
        if self.max_active == 0
            || self.max_active > 1_024
            || self.cancellation_poll.is_zero()
            || self.cancellation_poll > Duration::from_secs(1)
        {
            return Err(AiError::InvalidInput("execution queue configuration"));
        }
        Ok(self)
    }
}

impl Default for ExecutionQueueConfig {
    fn default() -> Self {
        Self {
            max_active: 8,
            waiting: FairQueueConfig::default(),
            cancellation_poll: Duration::from_millis(10),
        }
    }
}

/// Low-cardinality execution-admission snapshot.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ExecutionQueueSnapshot {
    /// Requests currently holding an execution slot.
    pub active: usize,
    /// Requests waiting for an execution slot.
    pub queued: usize,
    /// Requests admitted without waiting.
    pub immediate: u64,
    /// Requests admitted after waiting.
    pub waited: u64,
    /// Requests rejected because the bounded waiting queue was full.
    pub rejected_full: u64,
}

#[derive(Debug)]
struct State {
    active: usize,
    queue: FairQueue<()>,
    selected: BTreeSet<u64>,
    immediate: u64,
    waited: u64,
    rejected_full: u64,
}

/// Blocking, executor-neutral admission gate used by `AiRuntime`.
#[derive(Debug)]
pub(crate) struct ExecutionQueue {
    config: ExecutionQueueConfig,
    started: Instant,
    state: Mutex<State>,
    changed: Condvar,
}

impl ExecutionQueue {
    pub(crate) fn new(config: ExecutionQueueConfig) -> AiResult<Self> {
        let config = config.validate()?;
        Ok(Self {
            config,
            started: Instant::now(),
            state: Mutex::new(State {
                active: 0,
                queue: FairQueue::new(config.waiting)?,
                selected: BTreeSet::new(),
                immediate: 0,
                waited: 0,
                rejected_full: 0,
            }),
            changed: Condvar::new(),
        })
    }

    pub(crate) fn acquire(
        &self,
        priority: AiPriority,
        deadline: Option<Duration>,
        cancellation: CancellationToken,
    ) -> AiResult<ExecutionPermit<'_>> {
        if cancellation.is_cancelled() {
            return Err(AiError::Cancelled);
        }
        let admitted_at = Instant::now();
        let mut state = self.lock_state()?;
        if state.active.saturating_add(state.selected.len()) < self.config.max_active
            && state.queue.metrics().depth == 0
        {
            state.active = state.active.saturating_add(1);
            state.immediate = state.immediate.saturating_add(1);
            return Ok(ExecutionPermit { queue: self });
        }
        let now_ms = self.now_ms();
        let sequence =
            match state
                .queue
                .enqueue((), priority, now_ms, deadline, cancellation.clone())
            {
                QueueAdmission::Queued { sequence } => sequence,
                QueueAdmission::Rejected { reason, .. } => {
                    if reason == crate::QueueRejectionReason::Full {
                        state.rejected_full = state.rejected_full.saturating_add(1);
                        return Err(AiError::QueueFull);
                    }
                    return Err(match reason {
                        crate::QueueRejectionReason::Cancelled => AiError::Cancelled,
                        crate::QueueRejectionReason::Deadline => AiError::DeadlineExceeded,
                        crate::QueueRejectionReason::Full => AiError::QueueFull,
                    });
                }
            };
        loop {
            dispatch_available(&mut state, self.config.max_active, self.now_ms());
            if cancellation.is_cancelled() {
                state.queue.remove(sequence);
                state.selected.remove(&sequence);
                dispatch_available(&mut state, self.config.max_active, self.now_ms());
                self.changed.notify_all();
                return Err(AiError::Cancelled);
            }
            if deadline.is_some_and(|limit| admitted_at.elapsed() >= limit) {
                state.queue.remove(sequence);
                state.selected.remove(&sequence);
                dispatch_available(&mut state, self.config.max_active, self.now_ms());
                self.changed.notify_all();
                return Err(AiError::DeadlineExceeded);
            }
            if state.selected.remove(&sequence) {
                state.active = state.active.saturating_add(1);
                state.waited = state.waited.saturating_add(1);
                return Ok(ExecutionPermit { queue: self });
            }
            let waited = self
                .changed
                .wait_timeout(state, self.config.cancellation_poll)
                .map_err(|_| AiError::InternalState)?;
            state = waited.0;
        }
    }

    pub(crate) fn snapshot(&self) -> ExecutionQueueSnapshot {
        let Ok(state) = self.state.lock() else {
            return ExecutionQueueSnapshot::default();
        };
        ExecutionQueueSnapshot {
            active: state.active,
            queued: state.queue.metrics().depth,
            immediate: state.immediate,
            waited: state.waited,
            rejected_full: state.rejected_full,
        }
    }

    fn release(&self) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        state.active = state.active.saturating_sub(1);
        dispatch_available(&mut state, self.config.max_active, self.now_ms());
        self.changed.notify_all();
    }

    fn now_ms(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn lock_state(&self) -> AiResult<MutexGuard<'_, State>> {
        self.state.lock().map_err(|_| AiError::InternalState)
    }
}

fn dispatch_available(state: &mut State, max_active: usize, now_ms: u64) {
    while state.active.saturating_add(state.selected.len()) < max_active {
        let Some(work) = state.queue.dequeue(now_ms) else {
            break;
        };
        state.selected.insert(work.sequence);
    }
}

pub(crate) struct ExecutionPermit<'a> {
    queue: &'a ExecutionQueue,
}

impl Drop for ExecutionPermit<'_> {
    fn drop(&mut self) {
        self.queue.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn active_and_waiting_bounds_apply_real_backpressure() {
        let queue = Arc::new(
            ExecutionQueue::new(ExecutionQueueConfig {
                max_active: 1,
                waiting: FairQueueConfig {
                    capacity: 1,
                    ..FairQueueConfig::default()
                },
                cancellation_poll: Duration::from_millis(1),
            })
            .unwrap(),
        );
        let first = queue
            .acquire(AiPriority::Normal, None, CancellationToken::new())
            .unwrap();
        let waiting_queue = Arc::clone(&queue);
        let waiter = std::thread::spawn(move || {
            waiting_queue
                .acquire(AiPriority::High, None, CancellationToken::new())
                .map(drop)
        });
        let wait_deadline = Instant::now() + Duration::from_secs(1);
        while queue.snapshot().queued == 0 && Instant::now() < wait_deadline {
            std::thread::yield_now();
        }
        assert_eq!(queue.snapshot().queued, 1);
        assert!(matches!(
            queue.acquire(AiPriority::Normal, None, CancellationToken::new()),
            Err(AiError::QueueFull)
        ));
        drop(first);
        waiter.join().unwrap().unwrap();
        assert_eq!(queue.snapshot().active, 0);
        assert_eq!(queue.snapshot().rejected_full, 1);
    }

    #[test]
    fn queued_cancellation_releases_the_entry() {
        let queue = Arc::new(
            ExecutionQueue::new(ExecutionQueueConfig {
                max_active: 1,
                waiting: FairQueueConfig::default(),
                cancellation_poll: Duration::from_millis(1),
            })
            .unwrap(),
        );
        let first = queue
            .acquire(AiPriority::Normal, None, CancellationToken::new())
            .unwrap();
        let cancellation = CancellationToken::new();
        let waiter_token = cancellation.clone();
        let waiting_queue = Arc::clone(&queue);
        let waiter = std::thread::spawn(move || {
            waiting_queue
                .acquire(AiPriority::Normal, None, waiter_token)
                .map(drop)
        });
        let wait_deadline = Instant::now() + Duration::from_secs(1);
        while queue.snapshot().queued == 0 && Instant::now() < wait_deadline {
            std::thread::yield_now();
        }
        assert_eq!(queue.snapshot().queued, 1);
        cancellation.cancel();
        assert!(matches!(waiter.join().unwrap(), Err(AiError::Cancelled)));
        assert_eq!(queue.snapshot().queued, 0);
        drop(first);
    }

    #[test]
    fn cancellation_wins_after_dispatch_selection_but_before_permit_delivery() {
        let queue = Arc::new(
            ExecutionQueue::new(ExecutionQueueConfig {
                max_active: 1,
                waiting: FairQueueConfig::default(),
                cancellation_poll: Duration::from_millis(1),
            })
            .unwrap(),
        );
        let first = queue
            .acquire(AiPriority::Normal, None, CancellationToken::new())
            .unwrap();
        let cancellation = CancellationToken::new();
        let waiting_queue = Arc::clone(&queue);
        let waiter_token = cancellation.clone();
        let waiter = std::thread::spawn(move || {
            waiting_queue
                .acquire(AiPriority::Normal, None, waiter_token)
                .map(drop)
        });
        let wait_deadline = Instant::now() + Duration::from_secs(1);
        while queue.snapshot().queued == 0 && Instant::now() < wait_deadline {
            std::thread::yield_now();
        }
        let mut state = queue.state.lock().unwrap();
        state.active = 0;
        dispatch_available(&mut state, 1, queue.now_ms());
        assert_eq!(state.selected.len(), 1);
        cancellation.cancel();
        queue.changed.notify_all();
        drop(state);
        assert!(matches!(waiter.join().unwrap(), Err(AiError::Cancelled)));
        assert_eq!(queue.snapshot().active, 0);
        assert_eq!(queue.snapshot().queued, 0);
        drop(first);
    }
}
