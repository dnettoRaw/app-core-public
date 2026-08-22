// =============================================================================
//        #######
//     ###       ###     F: admission.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{
    AdmissionDecision, AiRequest, AiResult, DeviceId, DeviceKind, HardwareProbe, PlacementMetrics,
    ResourceEstimate, ResourceGovernor,
};
use std::time::Instant;

/// Monotonic time boundary used by deterministic admission tests.
pub trait AiClock: Send + Sync {
    /// Returns monotonic milliseconds from an implementation-defined origin.
    fn now_ms(&self) -> u64;
}

/// Production monotonic clock scoped to one AppCore AI runtime.
#[derive(Debug)]
pub struct SystemAiClock {
    started: Instant,
}

impl SystemAiClock {
    /// Starts a new monotonic clock.
    #[must_use]
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
        }
    }
}

impl Default for SystemAiClock {
    fn default() -> Self {
        Self::new()
    }
}

impl AiClock for SystemAiClock {
    fn now_ms(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

/// Deterministic manually advanced clock for simulations and tests.
#[derive(Debug, Default)]
pub struct StaticAiClock {
    now_ms: std::sync::atomic::AtomicU64,
}

impl StaticAiClock {
    /// Creates a clock at the supplied monotonic timestamp.
    #[must_use]
    pub fn new(now_ms: u64) -> Self {
        Self {
            now_ms: std::sync::atomic::AtomicU64::new(now_ms),
        }
    }

    /// Replaces the current monotonic timestamp.
    pub fn set(&self, now_ms: u64) {
        self.now_ms
            .store(now_ms, std::sync::atomic::Ordering::Release);
    }
}

impl AiClock for StaticAiClock {
    fn now_ms(&self) -> u64 {
        self.now_ms.load(std::sync::atomic::Ordering::Acquire)
    }
}

/// Resource-admission boundary used before a backend queue receives work.
pub trait ModelAdmission: Send + Sync {
    /// Applies policy to one backend-provided peak resource estimate.
    fn admit(&self, request: &AiRequest, estimate: ResourceEstimate)
        -> AiResult<AdmissionDecision>;

    /// Applies policy to one exact device; basic adapters may use aggregate admission.
    fn admit_on(
        &self,
        request: &AiRequest,
        estimate: ResourceEstimate,
        _kind: DeviceKind,
        _device: &DeviceId,
    ) -> AiResult<AdmissionDecision> {
        self.admit(request, estimate)
    }

    /// Supplies current hardware metrics for one target when this adapter owns them.
    fn placement_metrics(
        &self,
        _kind: DeviceKind,
        _device: &DeviceId,
    ) -> AiResult<Option<PlacementMetrics>> {
        Ok(None)
    }
}

/// Resource-governor adapter for model admission.
#[derive(Debug)]
pub struct GovernorAdmission<P, C> {
    governor: ResourceGovernor<P>,
    clock: C,
}

impl<P, C> GovernorAdmission<P, C> {
    /// Connects a resource governor to an injected monotonic clock.
    #[must_use]
    pub fn new(governor: ResourceGovernor<P>, clock: C) -> Self {
        Self { governor, clock }
    }
}

impl<P: HardwareProbe, C: AiClock> ModelAdmission for GovernorAdmission<P, C> {
    fn admit(
        &self,
        request: &AiRequest,
        estimate: ResourceEstimate,
    ) -> AiResult<AdmissionDecision> {
        self.governor
            .admit(request.options.resources, estimate, self.clock.now_ms())
    }

    fn admit_on(
        &self,
        request: &AiRequest,
        estimate: ResourceEstimate,
        kind: DeviceKind,
        device: &DeviceId,
    ) -> AiResult<AdmissionDecision> {
        self.governor.admit_on(
            request.options.resources,
            estimate,
            kind,
            device,
            self.clock.now_ms(),
        )
    }

    fn placement_metrics(
        &self,
        kind: DeviceKind,
        device: &DeviceId,
    ) -> AiResult<Option<PlacementMetrics>> {
        self.governor
            .placement_metrics(kind, device, self.clock.now_ms())
    }
}
