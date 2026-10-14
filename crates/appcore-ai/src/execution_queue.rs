// =============================================================================
//        #######
//     ###       ###     F: execution_queue.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Defines bounded execution queue contracts and behavior for this crate.

use crate::{
    AiError, AiPriority, AiResult, CancellationToken, FairQueue, FairQueueConfig, QueueAdmission,
};
use std::collections::BTreeSet;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Runtime concurrency and fair-waiting bounds applied around backend routing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionQueueConfig {
    /// Maximum requests concurrently executing model routes.
    pub max_active: usize,
    /// Priority, starvation and overload policy for waiting requests.
    pub waiting: FairQueueConfig,
    /// Maximum asynchronous timer interval before cancellation/deadline is rechecked.
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

/// Bounded admission gate with asynchronous waiting for `AiRuntime`.
#[derive(Debug)]
pub(crate) struct ExecutionQueue {
    config: ExecutionQueueConfig,
    started: Instant,
    state: Mutex<State>,
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
        })
    }

    #[cfg(test)]
    fn acquire(
        &self,
        priority: AiPriority,
        deadline: Option<Duration>,
        cancellation: CancellationToken,
    ) -> AiResult<ExecutionPermit<'_>> {
        crate::tests::block_on(self.acquire_async(priority, deadline, cancellation))
    }

    pub(crate) async fn acquire_async(
        &self,
        priority: AiPriority,
        deadline: Option<Duration>,
        cancellation: CancellationToken,
    ) -> AiResult<ExecutionPermit<'_>> {
        let started = Instant::now();
        if cancellation.is_cancelled() {
            return Err(AiError::Cancelled);
        }
        if deadline.is_some_and(|limit| limit.is_zero()) {
            return Err(AiError::DeadlineExceeded);
        }
        let sequence = {
            let mut state = self.lock_state()?;
            if state.active.saturating_add(state.selected.len()) < self.config.max_active
                && state.queue.metrics().depth == 0
            {
                state.active += 1;
                state.immediate = state.immediate.saturating_add(1);
                return Ok(ExecutionPermit { queue: self });
            }
            match state
                .queue
                .enqueue((), priority, self.now_ms(), deadline, cancellation.clone())
            {
                QueueAdmission::Queued { sequence } => sequence,
                QueueAdmission::Rejected { reason, .. } => {
                    return Err(match reason {
                        crate::QueueRejectionReason::Full => {
                            state.rejected_full = state.rejected_full.saturating_add(1);
                            AiError::QueueFull
                        }
                        crate::QueueRejectionReason::Cancelled => AiError::Cancelled,
                        crate::QueueRejectionReason::Deadline => AiError::DeadlineExceeded,
                    });
                }
            }
        };
        let mut waiting = WaitingExecution {
            queue: self,
            sequence: Some(sequence),
        };
        loop {
            {
                let mut state = self.lock_state()?;
                if cancellation.is_cancelled() {
                    return Err(AiError::Cancelled);
                }
                if deadline.is_some_and(|limit| started.elapsed() >= limit) {
                    return Err(AiError::DeadlineExceeded);
                }
                dispatch_available(&mut state, self.config.max_active, self.now_ms());
                if state.selected.remove(&sequence) {
                    state.active += 1;
                    state.waited = state.waited.saturating_add(1);
                    waiting.sequence = None;
                    return Ok(ExecutionPermit { queue: self });
                }
            }
            // A shared timer wakes this future without blocking an executor worker.
            let delay = deadline.map_or(self.config.cancellation_poll, |limit| {
                limit
                    .saturating_sub(started.elapsed())
                    .min(self.config.cancellation_poll)
            });
            futures_timer::Delay::new(delay).await;
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

struct WaitingExecution<'a> {
    queue: &'a ExecutionQueue,
    sequence: Option<u64>,
}

impl Drop for WaitingExecution<'_> {
    fn drop(&mut self) {
        if let Some(sequence) = self.sequence {
            if let Ok(mut state) = self.queue.state.lock() {
                state.queue.remove(sequence);
                state.selected.remove(&sequence);
                dispatch_available(
                    &mut state,
                    self.queue.config.max_active,
                    self.queue.now_ms(),
                );
            }
        }
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
    fn async_admission_yields_and_dropped_waiters_release_selection() {
        use std::future::Future;
        use std::task::{Context, Poll, Waker};
        let queue = ExecutionQueue::new(ExecutionQueueConfig {
            max_active: 1,
            ..Default::default()
        })
        .unwrap();
        let first = crate::tests::block_on(queue.acquire_async(
            AiPriority::Normal,
            None,
            CancellationToken::new(),
        ))
        .unwrap();
        let mut waiter =
            Box::pin(queue.acquire_async(AiPriority::Normal, None, CancellationToken::new()));
        let mut context = Context::from_waker(Waker::noop());
        assert!(matches!(waiter.as_mut().poll(&mut context), Poll::Pending));
        assert_eq!(queue.snapshot().queued, 1);
        drop(first);
        // Selection has happened, but dropping the future must still release it.
        drop(waiter);
        let next = crate::tests::block_on(queue.acquire_async(
            AiPriority::Normal,
            Some(Duration::from_secs(1)),
            CancellationToken::new(),
        ))
        .unwrap();
        assert_eq!(queue.snapshot().active, 1);
        drop(next);
        assert_eq!(queue.snapshot().queued, 0);
    }

    #[test]
    fn async_wait_rechecks_cancellation_and_deadline() {
        use std::future::Future;
        use std::task::{Context, Poll, Waker};
        let queue = ExecutionQueue::new(ExecutionQueueConfig {
            max_active: 1,
            ..Default::default()
        })
        .unwrap();
        let first = crate::tests::block_on(queue.acquire_async(
            AiPriority::Normal,
            None,
            CancellationToken::new(),
        ))
        .unwrap();
        let token = CancellationToken::new();
        let mut waiter = Box::pin(queue.acquire_async(AiPriority::Normal, None, token.clone()));
        assert!(matches!(
            waiter
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop())),
            Poll::Pending
        ));
        token.cancel();
        assert!(matches!(
            crate::tests::block_on(waiter),
            Err(AiError::Cancelled)
        ));
        assert!(matches!(
            crate::tests::block_on(queue.acquire_async(
                AiPriority::Normal,
                Some(Duration::from_millis(1)),
                CancellationToken::new()
            )),
            Err(AiError::DeadlineExceeded)
        ));
        assert_eq!(queue.snapshot().queued, 0);
        drop(first);
    }

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
        drop(state);
        assert!(matches!(waiter.join().unwrap(), Err(AiError::Cancelled)));
        assert_eq!(queue.snapshot().active, 0);
        assert_eq!(queue.snapshot().queued, 0);
        drop(first);
    }
}
