// =============================================================================
//        #######
//     ###       ###     F: batching.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{
    AiError, AiLatencyClass, AiResourceMode, AiResult, AiTask, BackendId, CancellationToken,
    CapabilityId, DeviceId, ModelId, ResourceBudget,
};
use std::collections::{BTreeMap, VecDeque};
use std::time::Duration;

/// Task class included in a strict batch compatibility key.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum BatchTaskClass {
    /// Text generation.
    GenerateText,
    /// Text transformation.
    TransformText,
    /// Text classification.
    ClassifyText,
    /// Extraction.
    Extract,
    /// Decision.
    Decide,
    /// Embedding.
    Embed,
    /// Image analysis.
    AnalyzeImage,
    /// Document analysis.
    AnalyzeDocument,
    /// Consumer-owned capability.
    Capability(CapabilityId),
}

impl From<&AiTask> for BatchTaskClass {
    fn from(task: &AiTask) -> Self {
        match task {
            AiTask::GenerateText => Self::GenerateText,
            AiTask::Chat => Self::GenerateText,
            AiTask::TransformText => Self::TransformText,
            AiTask::ClassifyText => Self::ClassifyText,
            AiTask::Extract => Self::Extract,
            AiTask::Decide => Self::Decide,
            AiTask::Embed => Self::Embed,
            AiTask::AnalyzeImage => Self::AnalyzeImage,
            AiTask::AnalyzeDocument => Self::AnalyzeDocument,
            AiTask::Capability(id) => Self::Capability(id.clone()),
        }
    }
}

/// Requests may share a batch only when this complete key matches.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct BatchKey {
    /// Logical model.
    pub model: ModelId,
    /// Concrete backend.
    pub backend: BackendId,
    /// Concrete device.
    pub device: DeviceId,
    /// Compatible task class.
    pub task: BatchTaskClass,
}

/// Global and per-key batching bounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DynamicBatcherConfig {
    /// Maximum concurrently materialized compatibility queues.
    pub max_queues: usize,
    /// Maximum items across every queue.
    pub max_total_items: usize,
    /// Maximum items in one compatibility queue.
    pub max_queue_depth: usize,
    /// Absolute batch-size ceiling.
    pub max_batch_size: usize,
    /// Maximum batching delay before an eligible flush.
    pub max_wait: Duration,
    /// Retry hint returned on bounded overload.
    pub overload_retry_after: Duration,
}

impl DynamicBatcherConfig {
    /// Validates all non-zero bounds and their relationship.
    pub fn validate(self) -> AiResult<Self> {
        if self.max_queues == 0
            || self.max_total_items == 0
            || self.max_queue_depth == 0
            || self.max_batch_size == 0
            || self.max_batch_size > self.max_queue_depth
            || self.max_wait.is_zero()
            || self.overload_retry_after.is_zero()
        {
            return Err(AiError::InvalidInput("dynamic batcher configuration"));
        }
        Ok(self)
    }
}

impl Default for DynamicBatcherConfig {
    fn default() -> Self {
        Self {
            max_queues: 32,
            max_total_items: 256,
            max_queue_depth: 64,
            max_batch_size: 16,
            max_wait: Duration::from_millis(10),
            overload_retry_after: Duration::from_millis(25),
        }
    }
}

/// Resource and latency signals used to derive an effective batch size.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatchPressure {
    /// Active resource mode.
    pub resource_mode: AiResourceMode,
    /// Resource-governor pressure flag.
    pub pressure_limited: bool,
    /// Memory currently available to this route when known.
    pub available_memory_bytes: Option<u64>,
    /// Device-local memory available to this route when known.
    pub available_vram_bytes: Option<u64>,
    /// Estimated incremental memory per item.
    pub estimated_item_memory_bytes: u64,
    /// Estimated incremental device-local memory per item.
    pub estimated_item_vram_bytes: u64,
    /// Current exact-device utilization when known.
    pub device_load_percent: Option<u8>,
}

impl BatchPressure {
    /// Builds adaptive pressure from one governor budget and exact route metrics.
    #[must_use]
    pub fn from_budget(
        resource_mode: AiResourceMode,
        budget: ResourceBudget,
        route: crate::PlacementMetrics,
        estimated_item_memory_bytes: u64,
        estimated_item_vram_bytes: u64,
    ) -> Self {
        Self {
            resource_mode,
            pressure_limited: budget.pressure_limited,
            available_memory_bytes: route.available_memory_bytes.or(budget.memory_bytes),
            available_vram_bytes: route.available_vram_bytes.or(budget.vram_bytes),
            estimated_item_memory_bytes,
            estimated_item_vram_bytes,
            device_load_percent: route.load_percent,
        }
    }
}

/// Route-specific latency, resource and backend ceilings for one batch flush.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatchDispatchPolicy {
    /// Resource and memory pressure supplied by the governor.
    pub pressure: BatchPressure,
    /// Request latency target used to trade delay for throughput.
    pub latency_class: AiLatencyClass,
    /// Optional tighter batch ceiling declared by the selected backend.
    pub backend_max_batch_size: Option<usize>,
}

impl From<BatchPressure> for BatchDispatchPolicy {
    fn from(pressure: BatchPressure) -> Self {
        Self {
            pressure,
            latency_class: AiLatencyClass::Throughput,
            backend_max_batch_size: None,
        }
    }
}

/// Stable reason batching admission rejected an item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BatchRejectionReason {
    /// A per-key or global item bound was reached.
    QueueFull,
    /// The bounded compatibility-queue count was reached.
    TooManyQueues,
    /// Cancellation was already requested.
    Cancelled,
    /// Deadline had already elapsed.
    Deadline,
}

/// Batching admission result that retains rejected caller work.
#[derive(Debug)]
pub enum BatchAdmission<T> {
    /// Item entered its compatibility queue.
    Queued {
        /// Stable item sequence.
        sequence: u64,
    },
    /// Item was rejected before ownership reached a backend.
    Rejected {
        /// Unconsumed caller item.
        item: T,
        /// Structured rejection reason.
        reason: BatchRejectionReason,
        /// Retry hint only for capacity pressure.
        retry_after: Option<Duration>,
    },
}

/// One dispatch item retaining its sequence for partial outcomes.
#[derive(Debug)]
pub struct BatchItem<T> {
    /// Stable admission sequence.
    pub sequence: u64,
    /// Caller work.
    pub item: T,
}

/// One ready strictly compatible batch.
#[derive(Debug)]
pub struct ReadyBatch<T> {
    /// Shared compatibility key.
    pub key: BatchKey,
    /// Bounded work items.
    pub items: Vec<BatchItem<T>>,
    /// Wait of the oldest included item.
    pub oldest_wait: Duration,
}

/// One independently successful or failed item from a dispatched batch.
#[derive(Debug)]
pub struct BatchItemOutcome<T> {
    /// Original queue sequence.
    pub sequence: u64,
    /// Backend-neutral per-item result.
    pub result: AiResult<T>,
}

/// Low-cardinality batching and backpressure counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BatcherMetrics {
    /// Accepted items.
    pub accepted: u64,
    /// Capacity rejections.
    pub rejected: u64,
    /// Cancelled items removed before dispatch.
    pub cancelled: u64,
    /// Expired items removed before dispatch.
    pub expired: u64,
    /// Dispatched batches.
    pub batches: u64,
    /// Dispatched items.
    pub items: u64,
    /// Per-item backend failures reported by callers.
    pub partial_failures: u64,
    /// Current total queued items.
    pub depth: usize,
}

#[derive(Debug)]
struct Entry<T> {
    sequence: u64,
    item: T,
    enqueued_ms: u64,
    deadline_ms: Option<u64>,
    cancellation: CancellationToken,
}

/// Bounded adaptive batch queues partitioned by model, backend, device and task.
#[derive(Debug)]
pub struct DynamicBatcher<T> {
    config: DynamicBatcherConfig,
    queues: BTreeMap<BatchKey, VecDeque<Entry<T>>>,
    next_sequence: u64,
    metrics: BatcherMetrics,
}

impl<T> DynamicBatcher<T> {
    /// Creates an empty adaptive batcher.
    pub fn new(config: DynamicBatcherConfig) -> AiResult<Self> {
        Ok(Self {
            config: config.validate()?,
            queues: BTreeMap::new(),
            next_sequence: 1,
            metrics: BatcherMetrics::default(),
        })
    }

    /// Enqueues work in its exact compatibility partition.
    pub fn enqueue(
        &mut self,
        key: BatchKey,
        item: T,
        now_ms: u64,
        deadline: Option<Duration>,
        cancellation: CancellationToken,
    ) -> BatchAdmission<T> {
        if cancellation.is_cancelled() {
            return rejected(item, BatchRejectionReason::Cancelled, None);
        }
        let deadline_ms = deadline.map(|value| now_ms.saturating_add(millis(value)));
        if deadline_ms.is_some_and(|value| value <= now_ms) {
            return rejected(item, BatchRejectionReason::Deadline, None);
        }
        if !self.queues.contains_key(&key) && self.queues.len() >= self.config.max_queues {
            self.metrics.rejected = self.metrics.rejected.saturating_add(1);
            return rejected(item, BatchRejectionReason::TooManyQueues, None);
        }
        let queue_depth = self.queues.get(&key).map_or(0, VecDeque::len);
        if self.metrics.depth >= self.config.max_total_items
            || queue_depth >= self.config.max_queue_depth
        {
            self.metrics.rejected = self.metrics.rejected.saturating_add(1);
            return rejected(
                item,
                BatchRejectionReason::QueueFull,
                Some(self.config.overload_retry_after),
            );
        }
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.queues.entry(key).or_default().push_back(Entry {
            sequence,
            item,
            enqueued_ms: now_ms,
            deadline_ms,
            cancellation,
        });
        self.metrics.accepted = self.metrics.accepted.saturating_add(1);
        self.metrics.depth = self.metrics.depth.saturating_add(1);
        BatchAdmission::Queued { sequence }
    }

    /// Takes a ready batch; `force` is intended for controlled shutdown or explicit flush.
    pub fn take_ready(
        &mut self,
        key: &BatchKey,
        now_ms: u64,
        pressure: BatchPressure,
        force: bool,
    ) -> Option<ReadyBatch<T>> {
        self.take_ready_with_policy(key, now_ms, pressure.into(), force)
    }

    /// Takes a ready batch using latency and backend-specific ceilings.
    pub fn take_ready_with_policy(
        &mut self,
        key: &BatchKey,
        now_ms: u64,
        policy: BatchDispatchPolicy,
        force: bool,
    ) -> Option<ReadyBatch<T>> {
        self.prune(key, now_ms);
        let batch_size = effective_batch_size(self.config.max_batch_size, policy);
        let queue = self.queues.get_mut(key)?;
        let oldest = queue.front()?;
        let oldest_wait_ms = now_ms.saturating_sub(oldest.enqueued_ms);
        let deadline_flush = oldest.deadline_ms.is_some_and(|deadline| {
            deadline <= now_ms.saturating_add(millis(self.config.max_wait))
        });
        if !force
            && queue.len() < batch_size
            && oldest_wait_ms < millis(self.config.max_wait)
            && !deadline_flush
        {
            return None;
        }
        let take = batch_size.min(queue.len());
        let items = queue
            .drain(..take)
            .map(|entry| BatchItem {
                sequence: entry.sequence,
                item: entry.item,
            })
            .collect::<Vec<_>>();
        if queue.is_empty() {
            self.queues.remove(key);
        }
        self.metrics.batches = self.metrics.batches.saturating_add(1);
        self.metrics.items = self
            .metrics
            .items
            .saturating_add(u64::try_from(items.len()).unwrap_or(u64::MAX));
        self.metrics.depth = self.metrics.depth.saturating_sub(items.len());
        Some(ReadyBatch {
            key: key.clone(),
            items,
            oldest_wait: Duration::from_millis(oldest_wait_ms),
        })
    }

    /// Records independently failed items from one backend batch result.
    pub fn record_outcomes<U>(&mut self, outcomes: &[BatchItemOutcome<U>]) {
        let failures = outcomes.iter().filter(|item| item.result.is_err()).count();
        self.metrics.partial_failures = self
            .metrics
            .partial_failures
            .saturating_add(u64::try_from(failures).unwrap_or(u64::MAX));
    }

    /// Returns bounded batching counters.
    #[must_use]
    pub fn metrics(&self) -> BatcherMetrics {
        self.metrics
    }

    fn prune(&mut self, key: &BatchKey, now_ms: u64) {
        let Some(queue) = self.queues.get_mut(key) else {
            return;
        };
        let before = queue.len();
        queue.retain(|entry| {
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
        self.metrics.depth = self
            .metrics
            .depth
            .saturating_sub(before.saturating_sub(queue.len()));
        if queue.is_empty() {
            self.queues.remove(key);
        }
    }
}

fn effective_batch_size(maximum: usize, policy: BatchDispatchPolicy) -> usize {
    let maximum = maximum.min(policy.backend_max_batch_size.unwrap_or(maximum).max(1));
    let latency_size = match policy.latency_class {
        AiLatencyClass::Interactive => 1,
        AiLatencyClass::Balanced => maximum.div_ceil(2),
        AiLatencyClass::Throughput | AiLatencyClass::Background => maximum,
    };
    let mode_size = match policy.pressure.resource_mode {
        AiResourceMode::Eco => 1,
        AiResourceMode::Balanced | AiResourceMode::Custom(_) => latency_size.div_ceil(2),
        AiResourceMode::Performance | AiResourceMode::Unrestricted => latency_size,
    };
    let pressure_size = if policy.pressure.pressure_limited {
        mode_size.div_ceil(2)
    } else {
        mode_size
    };
    let memory_size = match (
        policy.pressure.available_memory_bytes,
        policy.pressure.estimated_item_memory_bytes,
    ) {
        (Some(available), per_item) if per_item > 0 => {
            usize::try_from(available / per_item).unwrap_or(usize::MAX)
        }
        _ => pressure_size,
    };
    let vram_size = match (
        policy.pressure.available_vram_bytes,
        policy.pressure.estimated_item_vram_bytes,
    ) {
        (Some(available), per_item) if per_item > 0 => {
            usize::try_from(available / per_item).unwrap_or(usize::MAX)
        }
        _ => pressure_size,
    };
    let load_size = if policy
        .pressure
        .device_load_percent
        .is_some_and(|load| load >= 90)
    {
        pressure_size.div_ceil(2)
    } else {
        pressure_size
    };
    pressure_size
        .min(memory_size)
        .min(vram_size)
        .min(load_size)
        .max(1)
}

fn rejected<T>(
    item: T,
    reason: BatchRejectionReason,
    retry_after: Option<Duration>,
) -> BatchAdmission<T> {
    BatchAdmission::Rejected {
        item,
        reason,
        retry_after,
    }
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
