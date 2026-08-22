// =============================================================================
//        #######
//     ###       ###     F: queue.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{AiError, AiPriority, AiResult, CancellationToken};
use std::collections::VecDeque;
use std::time::Duration;

/// Bounds and fairness policy for one scheduler queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FairQueueConfig {
    /// Maximum queued items.
    pub capacity: usize,
    /// Waiting time after which the oldest lower-priority item is promoted.
    pub starvation_after: Duration,
    /// Retry hint returned when the queue is full.
    pub overload_retry_after: Duration,
}

impl FairQueueConfig {
    /// Validates non-zero queue and timing bounds.
    pub fn validate(self) -> AiResult<Self> {
        if self.capacity == 0
            || self.starvation_after.is_zero()
            || self.overload_retry_after.is_zero()
        {
            return Err(AiError::InvalidInput("fair queue configuration"));
        }
        Ok(self)
    }
}

impl Default for FairQueueConfig {
    fn default() -> Self {
        Self {
            capacity: 128,
            starvation_after: Duration::from_secs(2),
            overload_retry_after: Duration::from_millis(50),
        }
    }
}

/// Stable reason an item was not admitted to a queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueRejectionReason {
    /// Queue capacity was exhausted.
    Full,
    /// Cancellation was already requested.
    Cancelled,
    /// Deadline had already elapsed.
    Deadline,
}

/// Result of one queue admission, retaining rejected work for the caller.
#[derive(Debug)]
pub enum QueueAdmission<T> {
    /// Item entered the queue.
    Queued {
        /// Monotonic queue sequence.
        sequence: u64,
    },
    /// Item was rejected without being dispatched.
    Rejected {
        /// Unconsumed caller item.
        item: T,
        /// Structured backpressure reason.
        reason: QueueRejectionReason,
        /// Retry hint only when retry can be meaningful.
        retry_after: Option<Duration>,
    },
}

/// One item selected for dispatch.
#[derive(Debug)]
pub struct QueuedWork<T> {
    /// Caller item.
    pub item: T,
    /// Original priority.
    pub priority: AiPriority,
    /// Time spent queued.
    pub waited: Duration,
    /// Stable admission sequence.
    pub sequence: u64,
}

/// Low-cardinality queue counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FairQueueMetrics {
    /// Accepted items.
    pub accepted: u64,
    /// Capacity rejections.
    pub rejected_full: u64,
    /// Items discarded before dispatch because cancellation was observed.
    pub cancelled: u64,
    /// Items discarded before dispatch because their deadline elapsed.
    pub expired: u64,
    /// Items promoted by starvation prevention.
    pub promoted: u64,
    /// Current queue depth.
    pub depth: usize,
}

#[derive(Debug)]
struct Entry<T> {
    item: T,
    priority: AiPriority,
    enqueued_ms: u64,
    deadline_ms: Option<u64>,
    cancellation: CancellationToken,
    sequence: u64,
}

/// Bounded priority queue with cancellation, deadlines and starvation prevention.
#[derive(Debug)]
pub struct FairQueue<T> {
    config: FairQueueConfig,
    entries: VecDeque<Entry<T>>,
    next_sequence: u64,
    metrics: FairQueueMetrics,
}

impl<T> FairQueue<T> {
    /// Creates an empty bounded queue.
    pub fn new(config: FairQueueConfig) -> AiResult<Self> {
        Ok(Self {
            config: config.validate()?,
            entries: VecDeque::new(),
            next_sequence: 1,
            metrics: FairQueueMetrics::default(),
        })
    }

    /// Attempts to enqueue one item at an injected monotonic timestamp.
    pub fn enqueue(
        &mut self,
        item: T,
        priority: AiPriority,
        now_ms: u64,
        deadline: Option<Duration>,
        cancellation: CancellationToken,
    ) -> QueueAdmission<T> {
        if cancellation.is_cancelled() {
            return QueueAdmission::Rejected {
                item,
                reason: QueueRejectionReason::Cancelled,
                retry_after: None,
            };
        }
        let deadline_ms = deadline.map(|value| now_ms.saturating_add(millis(value)));
        if deadline_ms.is_some_and(|deadline| deadline <= now_ms) {
            return QueueAdmission::Rejected {
                item,
                reason: QueueRejectionReason::Deadline,
                retry_after: None,
            };
        }
        if self.entries.len() >= self.config.capacity {
            self.metrics.rejected_full = self.metrics.rejected_full.saturating_add(1);
            return QueueAdmission::Rejected {
                item,
                reason: QueueRejectionReason::Full,
                retry_after: Some(self.config.overload_retry_after),
            };
        }
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.entries.push_back(Entry {
            item,
            priority,
            enqueued_ms: now_ms,
            deadline_ms,
            cancellation,
            sequence,
        });
        self.metrics.accepted = self.metrics.accepted.saturating_add(1);
        self.metrics.depth = self.entries.len();
        QueueAdmission::Queued { sequence }
    }

    /// Selects the next live item, pruning cancelled and expired work first.
    pub fn dequeue(&mut self, now_ms: u64) -> Option<QueuedWork<T>> {
        self.prune(now_ms);
        let starvation_ms = millis(self.config.starvation_after);
        let promoted = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| now_ms.saturating_sub(entry.enqueued_ms) >= starvation_ms)
            .min_by_key(|(_, entry)| entry.sequence)
            .map(|(index, _)| index);
        let index = promoted.unwrap_or_else(|| {
            self.entries
                .iter()
                .enumerate()
                .max_by(|(_, left), (_, right)| {
                    left.priority
                        .cmp(&right.priority)
                        .then_with(|| right.sequence.cmp(&left.sequence))
                })
                .map_or(0, |(index, _)| index)
        });
        if promoted.is_some() {
            self.metrics.promoted = self.metrics.promoted.saturating_add(1);
        }
        let entry = self.entries.remove(index)?;
        self.metrics.depth = self.entries.len();
        Some(QueuedWork {
            item: entry.item,
            priority: entry.priority,
            waited: Duration::from_millis(now_ms.saturating_sub(entry.enqueued_ms)),
            sequence: entry.sequence,
        })
    }

    /// Returns a snapshot of bounded queue counters.
    #[must_use]
    pub fn metrics(&self) -> FairQueueMetrics {
        self.metrics
    }

    /// Removes one still-queued item by admission sequence.
    ///
    /// Queue owners use this when a waiter observes cancellation or a deadline
    /// before dispatch. A selected or unknown sequence returns `None`.
    pub fn remove(&mut self, sequence: u64) -> Option<T> {
        let index = self
            .entries
            .iter()
            .position(|entry| entry.sequence == sequence)?;
        let entry = self.entries.remove(index)?;
        self.metrics.cancelled = self.metrics.cancelled.saturating_add(1);
        self.metrics.depth = self.entries.len();
        Some(entry.item)
    }

    fn prune(&mut self, now_ms: u64) {
        self.entries.retain(|entry| {
            if entry.cancellation.is_cancelled() {
                self.metrics.cancelled = self.metrics.cancelled.saturating_add(1);
                false
            } else if entry.deadline_ms.is_some_and(|deadline| deadline <= now_ms) {
                self.metrics.expired = self.metrics.expired.saturating_add(1);
                false
            } else {
                true
            }
        });
        self.metrics.depth = self.entries.len();
    }
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
