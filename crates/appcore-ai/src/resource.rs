// =============================================================================
//        #######
//     ###       ###     F: resource.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{AiResult, DeviceId};
use std::time::Duration;

/// Hardware class exposed to backend-neutral placement.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DeviceKind {
    /// General-purpose processor.
    Cpu,
    /// Graphics processor.
    Gpu,
    /// Neural processing accelerator.
    Npu,
}

/// Coarse device class used without leaking a vendor SDK type.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DeviceClass {
    /// General-purpose CPU execution.
    Cpu,
    /// GPU integrated with the host memory subsystem.
    IntegratedGpu,
    /// GPU with a dedicated device-memory pool.
    DiscreteGpu,
    /// Neural or matrix accelerator.
    NeuralAccelerator,
    /// The platform cannot classify the device reliably.
    #[default]
    Unknown,
}

/// Memory ownership semantics relevant to model-fit calculations.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DeviceMemoryKind {
    /// Device memory is separate from system RAM.
    Dedicated,
    /// CPU and accelerator consume the same system-memory pool.
    Unified,
    /// The platform cannot determine the memory topology.
    #[default]
    Unknown,
}

/// Backend API that the operating system can identify without opening a model runtime.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DeviceApi {
    /// Portable CPU execution.
    Cpu,
    /// Apple Metal.
    Metal,
    /// NVIDIA CUDA driver family.
    Cuda,
    /// AMD ROCm driver family.
    Rocm,
    /// Microsoft DirectML.
    DirectMl,
    /// Vulkan compute.
    Vulkan,
}

/// Bounded backend-neutral device capabilities.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeviceCapabilities {
    /// Coarse hardware class.
    pub class: DeviceClass,
    /// Whether memory is dedicated, unified or unknown.
    pub memory_kind: DeviceMemoryKind,
    /// Driver/API families indicated by the platform probe.
    ///
    /// The selected inference backend must still validate that its concrete
    /// runtime, device and model combination is usable.
    pub compatible_apis: Vec<DeviceApi>,
}

/// Coarse thermal pressure from a trustworthy platform API.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ThermalPressure {
    /// Platform reports normal thermal conditions.
    Nominal,
    /// Platform reports elevated but non-critical pressure.
    Fair,
    /// Platform recommends reducing work.
    Serious,
    /// Platform requires aggressive load reduction.
    Critical,
    /// No reliable platform signal is available.
    #[default]
    Unknown,
}

/// One observed local accelerator or CPU device.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceSnapshot {
    /// Stable device identity for the lifetime of the probe.
    pub id: DeviceId,
    /// Device hardware class.
    pub kind: DeviceKind,
    /// Backend-neutral scheduling capabilities.
    pub capabilities: DeviceCapabilities,
    /// Total device-local memory when known.
    pub total_memory_bytes: Option<u64>,
    /// Currently available device-local memory when known.
    pub available_memory_bytes: Option<u64>,
    /// Currently used device-local memory when known.
    pub used_memory_bytes: Option<u64>,
    /// Observed utilization percentage when known.
    pub utilization_percent: Option<u8>,
    /// Whether the device is currently eligible for new work.
    pub healthy: bool,
}

/// Probe subsystem whose data could not be collected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceProbeComponent {
    /// Whole-machine CPU counters or topology.
    Cpu,
    /// Whole-machine memory counters.
    Memory,
    /// Optional accelerator discovery or dynamic data.
    Accelerator,
}

/// Stable failure class without raw OS paths or driver messages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceProbeFailureKind {
    /// The platform does not expose the metric.
    Unavailable,
    /// The current process lacks read permission.
    PermissionDenied,
    /// A device driver failed or a device disappeared.
    Driver,
    /// The operating system returned malformed or inconsistent data.
    InvalidData,
}

/// One redacted best-effort probe failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceProbeFailure {
    /// Affected subsystem.
    pub component: ResourceProbeComponent,
    /// Stable failure category.
    pub kind: ResourceProbeFailureKind,
}

/// Overall usefulness of one resource snapshot.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ResourceProbeStatus {
    /// Core CPU and memory data are usable.
    Healthy,
    /// Some requested data is unavailable; unknown fields remain unknown.
    Degraded,
    /// The platform boundary has not been implemented for this target.
    #[default]
    Unsupported,
}

/// Bounded observation used by the resource governor.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResourceSnapshot {
    /// Wall-clock capture time for diagnostics and expiring advertisements.
    pub captured_at_unix_ms: Option<u64>,
    /// Logical CPU count when known.
    pub logical_cpus: Option<usize>,
    /// Physical CPU count when known.
    pub physical_cpus: Option<usize>,
    /// Whole-machine CPU load percentage when known.
    pub cpu_load_percent: Option<u8>,
    /// Current process CPU use as a percentage of whole-machine capacity.
    pub process_cpu_percent: Option<u8>,
    /// Total system RAM when known.
    pub total_memory_bytes: Option<u64>,
    /// Available system RAM when known.
    pub available_memory_bytes: Option<u64>,
    /// Used system RAM derived from platform total and available semantics.
    pub used_memory_bytes: Option<u64>,
    /// OS-reported memory-stall pressure percentage when available.
    pub memory_pressure_percent: Option<u8>,
    /// Known compute devices.
    pub devices: Vec<DeviceSnapshot>,
    /// Current AI queue depth.
    pub queue_depth: usize,
    /// Current admitted AI jobs.
    pub active_jobs: usize,
    /// Reliable thermal signal when available.
    pub thermal_pressure: ThermalPressure,
    /// Whether the core platform sample is usable.
    pub status: ResourceProbeStatus,
    /// Bounded redacted failures; unknown is never converted into zero usage.
    pub failures: Vec<ResourceProbeFailure>,
}

impl ResourceSnapshot {
    /// Returns wall-clock age when both timestamps share the Unix time domain.
    #[must_use]
    pub fn age_at(&self, now_unix_ms: u64) -> Option<Duration> {
        self.captured_at_unix_ms
            .map(|captured| Duration::from_millis(now_unix_ms.saturating_sub(captured)))
    }

    /// Finds one healthy exact device snapshot.
    #[must_use]
    pub fn device(&self, id: &DeviceId) -> Option<&DeviceSnapshot> {
        self.devices
            .iter()
            .find(|device| device.healthy && &device.id == id)
    }
}

/// Boundary used to observe hardware without making real hardware a test dependency.
pub trait HardwareProbe: Send + Sync {
    /// Produces one best-effort resource snapshot.
    fn sample(&self) -> AiResult<ResourceSnapshot>;
}

/// Bounded result returned by an optional accelerator boundary.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AcceleratorSample {
    /// Independently addressable devices.
    pub devices: Vec<DeviceSnapshot>,
    /// Redacted failures that did not invalidate CPU execution.
    pub failures: Vec<ResourceProbeFailure>,
}

/// Small read-only extension point for vendor or platform accelerators.
pub trait AcceleratorProbe: Send + Sync {
    /// Samples known accelerators without changing clocks, fans or power limits.
    fn sample_accelerators(&self) -> AcceleratorSample;
}

/// Local policy controlling resources announced to authenticated swarm peers.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AiContributionPolicy {
    /// Whether this node donates compute.
    pub contribute_compute: bool,
    /// Whether this node donates artifact storage.
    pub contribute_storage: bool,
    /// Maximum donated CPU percentage.
    pub max_cpu_percent: u8,
    /// Maximum donated GPU percentage.
    pub max_gpu_percent: u8,
    /// Maximum RAM exposed to contributed work or cache.
    pub max_memory_bytes: u64,
    /// Maximum device-local memory exposed to contributed compute.
    pub max_vram_bytes: u64,
    /// Maximum persistent storage exposed to contributed artifacts.
    pub max_storage_bytes: u64,
    /// Maximum donated workers.
    pub max_workers: usize,
    /// Maximum concurrent donated jobs.
    pub max_concurrent_jobs: usize,
}

/// Resource-governor sampling and stability policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceGovernorConfig {
    /// Minimum delay between physical probe samples.
    pub sampling_interval: Duration,
    /// Consecutive samples required to enter or leave pressure throttling.
    pub hysteresis_samples: u8,
    /// Maximum logical workers even in unrestricted mode.
    pub max_workers: usize,
    /// Maximum admitted jobs even in unrestricted mode.
    pub max_concurrent_jobs: usize,
    /// Extra RAM always reserved by AppCore policy.
    pub reserved_memory_bytes: u64,
    /// Maximum queue depth before pressure is declared.
    pub pressure_queue_depth: usize,
}

impl Default for ResourceGovernorConfig {
    fn default() -> Self {
        Self {
            sampling_interval: Duration::from_secs(1),
            hysteresis_samples: 3,
            max_workers: 64,
            max_concurrent_jobs: 8,
            reserved_memory_bytes: 256 * 1024 * 1024,
            pressure_queue_depth: 32,
        }
    }
}

/// Resource amount made available to one placement domain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceBudget {
    /// Maximum CPU utilization percentage.
    pub cpu_percent: u8,
    /// Maximum GPU utilization percentage.
    pub gpu_percent: u8,
    /// RAM available to admitted work when known.
    pub memory_bytes: Option<u64>,
    /// VRAM available to admitted work when known.
    pub vram_bytes: Option<u64>,
    /// Persistent artifact-storage contribution.
    pub storage_bytes: u64,
    /// Maximum workers.
    pub workers: usize,
    /// Maximum concurrent jobs.
    pub concurrent_jobs: usize,
    /// Whether hysteresis currently applies pressure reduction.
    pub pressure_limited: bool,
}

/// Separate budgets for local work and swarm contribution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceBudgetPair {
    /// Budget retained for local execution.
    pub local: ResourceBudget,
    /// Budget that may be advertised to authorized peers.
    pub contribution: ResourceBudget,
}

/// Low-cardinality resource-governor counters and current gauges.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResourceGovernorMetrics {
    /// Physical calls made to the configured probe.
    pub samples: u64,
    /// Failed physical samples.
    pub sample_failures: u64,
    /// Snapshot reads served by the governor cache.
    pub cache_hits: u64,
    /// Admission decisions that deferred or rejected work.
    pub admission_denied: u64,
    /// Devices in the latest valid snapshot.
    pub device_count: usize,
    /// Latest CPU load when known.
    pub cpu_pressure_percent: Option<u8>,
    /// Latest memory pressure signal when known.
    pub memory_pressure_percent: Option<u8>,
    /// Age in the injected monotonic time domain.
    pub snapshot_age: Option<Duration>,
}

/// Conservative cost estimate required before admission.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResourceEstimate {
    /// Peak CPU percentage requested.
    pub cpu_percent: u8,
    /// Peak GPU percentage requested.
    pub gpu_percent: u8,
    /// Peak RAM bytes requested.
    pub memory_bytes: u64,
    /// Peak VRAM bytes requested.
    pub vram_bytes: u64,
    /// Worker slots requested.
    pub workers: usize,
}

/// Explicit model, runtime and batch components used to build a peak estimate.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResourceEstimateBreakdown {
    /// Model weights and persistent host-side state.
    pub model_memory_bytes: u64,
    /// Backend runtime, context and scratch overhead in host RAM.
    pub runtime_memory_bytes: u64,
    /// Peak incremental host RAM for the admitted batch.
    pub batch_memory_bytes: u64,
    /// Model weights placed in dedicated or unified accelerator memory.
    pub model_vram_bytes: u64,
    /// Accelerator runtime and context overhead.
    pub runtime_vram_bytes: u64,
    /// Peak incremental accelerator memory for the admitted batch.
    pub batch_vram_bytes: u64,
    /// Peak CPU percentage.
    pub cpu_percent: u8,
    /// Peak GPU percentage.
    pub gpu_percent: u8,
    /// Worker slots.
    pub workers: usize,
}

impl ResourceEstimateBreakdown {
    /// Produces the saturating peak estimate consumed by admission.
    #[must_use]
    pub fn peak(self) -> ResourceEstimate {
        ResourceEstimate {
            cpu_percent: self.cpu_percent,
            gpu_percent: self.gpu_percent,
            memory_bytes: self
                .model_memory_bytes
                .saturating_add(self.runtime_memory_bytes)
                .saturating_add(self.batch_memory_bytes),
            vram_bytes: self
                .model_vram_bytes
                .saturating_add(self.runtime_vram_bytes)
                .saturating_add(self.batch_vram_bytes),
            workers: self.workers,
        }
    }
}

/// Structured resource admission reason.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdmissionReason {
    /// Required capacity is unknown and cannot be assumed.
    UnknownCapacity,
    /// The requested device is absent or no longer healthy.
    DeviceUnavailable,
    /// CPU budget is insufficient.
    CpuPressure,
    /// GPU budget is insufficient.
    GpuPressure,
    /// RAM budget is insufficient.
    MemoryPressure,
    /// VRAM budget is insufficient.
    VramPressure,
    /// Worker budget is insufficient.
    WorkerLimit,
    /// Concurrent-job budget is exhausted.
    ConcurrentJobLimit,
    /// Thermal pressure requires deferral.
    ThermalPressure,
}

/// Resource-governor admission result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdmissionDecision {
    /// Work may enter the bounded scheduler.
    Admit {
        /// Budget used for the decision.
        budget: ResourceBudget,
    },
    /// Work may be retried after resource pressure changes.
    Defer {
        /// Structured deferral reason.
        reason: AdmissionReason,
        /// Minimum suggested retry delay.
        retry_after: Duration,
    },
    /// Work is incompatible with the configured resource policy.
    Reject {
        /// Structured rejection reason.
        reason: AdmissionReason,
    },
}
