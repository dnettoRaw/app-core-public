// =============================================================================
//        #######
//     ###       ###     F: governor.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::governor_policy::{
    admission, contribution_budget, device_vram, duration_ms, local_budget, validate_estimate,
};
use crate::{
    AdmissionDecision, AdmissionReason, AiContributionPolicy, AiError, AiResourceMode, AiResult,
    DeviceId, DeviceKind, DeviceMemoryKind, HardwareProbe, PlacementMetrics, ResidencyTier,
    ResourceBudgetPair, ResourceEstimate, ResourceGovernorConfig, ResourceGovernorMetrics,
    ResourceSnapshot, ThermalPressure, TierCapacity,
};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

#[derive(Debug, Default)]
struct GovernorState {
    cached: Option<(u64, ResourceSnapshot)>,
    last_failure: Option<(u64, AiError)>,
    sampling: bool,
    pressure_samples: u8,
    recovery_samples: u8,
    pressure_limited: bool,
    samples: u64,
    sample_failures: u64,
    cache_hits: u64,
    admission_denied: u64,
}

/// Adaptive, non-polling resource budget and admission controller.
#[derive(Debug)]
pub struct ResourceGovernor<P> {
    probe: P,
    config: ResourceGovernorConfig,
    contribution: AiContributionPolicy,
    state: Mutex<GovernorState>,
    sampled: Condvar,
}

impl<P: HardwareProbe> ResourceGovernor<P> {
    /// Builds a governor after validating every static policy bound.
    pub fn new(
        probe: P,
        config: ResourceGovernorConfig,
        contribution: AiContributionPolicy,
    ) -> AiResult<Self> {
        validate_config(config, contribution)?;
        Ok(Self {
            probe,
            config,
            contribution,
            state: Mutex::new(GovernorState::default()),
            sampled: Condvar::new(),
        })
    }

    /// Samples at most once per configured interval and calculates both budgets.
    pub fn budgets(&self, mode: AiResourceMode, now_ms: u64) -> AiResult<ResourceBudgetPair> {
        let (snapshot, pressure_limited) = self.sample(now_ms)?;
        let local = local_budget(&snapshot, self.config, mode, pressure_limited);
        let contribution = contribution_budget(local, self.contribution);
        Ok(ResourceBudgetPair {
            local,
            contribution,
        })
    }

    /// Returns the latest valid snapshot, sampling at most once per interval.
    pub fn snapshot(&self, now_ms: u64) -> AiResult<ResourceSnapshot> {
        self.sample(now_ms).map(|(snapshot, _)| snapshot)
    }

    /// Returns low-cardinality counters and current resource gauges.
    pub fn metrics(&self, now_ms: u64) -> AiResult<ResourceGovernorMetrics> {
        let state = self.state.lock().map_err(|_| AiError::InternalState)?;
        let (sampled_at, snapshot) = state
            .cached
            .as_ref()
            .map_or((None, None), |(at, snapshot)| (Some(*at), Some(snapshot)));
        Ok(ResourceGovernorMetrics {
            samples: state.samples,
            sample_failures: state.sample_failures,
            cache_hits: state.cache_hits,
            admission_denied: state.admission_denied,
            device_count: snapshot.map_or(0, |value| value.devices.len()),
            cpu_pressure_percent: snapshot.and_then(|value| value.cpu_load_percent),
            memory_pressure_percent: snapshot.and_then(|value| value.memory_pressure_percent),
            snapshot_age: sampled_at.map(|at| Duration::from_millis(now_ms.saturating_sub(at))),
        })
    }

    /// Applies local admission policy to a declared resource estimate.
    pub fn admit(
        &self,
        mode: AiResourceMode,
        estimate: ResourceEstimate,
        now_ms: u64,
    ) -> AiResult<AdmissionDecision> {
        validate_estimate(estimate)?;
        let (snapshot, pressure_limited) = self.sample(now_ms)?;
        let budget = local_budget(&snapshot, self.config, mode, pressure_limited);
        let decision = admission(
            &snapshot,
            budget,
            estimate,
            self.config.sampling_interval,
            None,
        );
        self.record_admission(decision)?;
        Ok(decision)
    }

    /// Applies admission to one exact device, preventing aggregate multi-GPU fit.
    pub fn admit_on(
        &self,
        mode: AiResourceMode,
        estimate: ResourceEstimate,
        kind: DeviceKind,
        device: &DeviceId,
        now_ms: u64,
    ) -> AiResult<AdmissionDecision> {
        validate_estimate(estimate)?;
        let (snapshot, pressure_limited) = self.sample(now_ms)?;
        let mut budget = local_budget(&snapshot, self.config, mode, pressure_limited);
        let memory_kind = if kind == DeviceKind::Cpu {
            budget.vram_bytes = Some(0);
            None
        } else if let Some(snapshot_device) = snapshot.device(device) {
            budget.vram_bytes = device_vram(snapshot_device, mode);
            Some(snapshot_device.capabilities.memory_kind)
        } else {
            let decision = AdmissionDecision::Defer {
                reason: AdmissionReason::DeviceUnavailable,
                retry_after: self.config.sampling_interval,
            };
            self.record_admission(decision)?;
            return Ok(decision);
        };
        let decision = admission(
            &snapshot,
            budget,
            estimate,
            self.config.sampling_interval,
            memory_kind,
        );
        self.record_admission(decision)?;
        Ok(decision)
    }

    /// Returns hardware observations for one exact local placement target.
    pub fn placement_metrics(
        &self,
        kind: DeviceKind,
        device: &DeviceId,
        now_ms: u64,
    ) -> AiResult<Option<PlacementMetrics>> {
        let snapshot = self.snapshot(now_ms)?;
        if kind == DeviceKind::Cpu {
            return Ok(Some(PlacementMetrics {
                load_percent: snapshot.cpu_load_percent,
                available_memory_bytes: snapshot.available_memory_bytes,
                available_vram_bytes: Some(0),
                ..PlacementMetrics::default()
            }));
        }
        Ok(snapshot.device(device).map(|value| PlacementMetrics {
            load_percent: value.utilization_percent,
            available_memory_bytes: snapshot.available_memory_bytes,
            available_vram_bytes: (value.capabilities.memory_kind == DeviceMemoryKind::Dedicated)
                .then_some(value.available_memory_bytes)
                .flatten(),
            ..PlacementMetrics::default()
        }))
    }

    /// Derives non-overlapping residency tiers from the current hardware budget.
    pub fn residency_capacities(
        &self,
        mode: AiResourceMode,
        now_ms: u64,
    ) -> AiResult<Vec<TierCapacity>> {
        let (snapshot, pressure_limited) = self.sample(now_ms)?;
        let budget = local_budget(&snapshot, self.config, mode, pressure_limited);
        let mut capacities = Vec::new();
        if let Some(memory) = budget.memory_bytes.filter(|value| *value > 0) {
            capacities.push(TierCapacity {
                tier: ResidencyTier::Memory,
                capacity_bytes: memory,
            });
        }
        for device in snapshot.devices.iter().filter(|device| {
            device.kind == DeviceKind::Gpu
                && device.healthy
                && device.capabilities.memory_kind == DeviceMemoryKind::Dedicated
        }) {
            if let Some(capacity) = device_vram(device, mode).filter(|value| *value > 0) {
                capacities.push(TierCapacity {
                    tier: ResidencyTier::Vram(device.id.clone()),
                    capacity_bytes: capacity,
                });
            }
        }
        Ok(capacities)
    }

    fn record_admission(&self, decision: AdmissionDecision) -> AiResult<()> {
        if !matches!(decision, AdmissionDecision::Admit { .. }) {
            let mut state = self.state.lock().map_err(|_| AiError::InternalState)?;
            state.admission_denied = state.admission_denied.saturating_add(1);
        }
        Ok(())
    }

    fn sample(&self, now_ms: u64) -> AiResult<(ResourceSnapshot, bool)> {
        let interval_ms = duration_ms(self.config.sampling_interval);
        let mut state = self.state.lock().map_err(|_| AiError::InternalState)?;
        loop {
            if let Some((failed_at, error)) = &state.last_failure {
                if now_ms >= *failed_at && now_ms - *failed_at < interval_ms {
                    return Err(error.clone());
                }
            }
            let cached = state.cached.as_ref().and_then(|(sampled_at, snapshot)| {
                (now_ms >= *sampled_at && now_ms - *sampled_at < interval_ms)
                    .then(|| snapshot.clone())
            });
            if let Some(snapshot) = cached {
                state.cache_hits = state.cache_hits.saturating_add(1);
                return Ok((snapshot, state.pressure_limited));
            }
            if !state.sampling {
                state.sampling = true;
                break;
            }
            state = self
                .sampled
                .wait(state)
                .map_err(|_| AiError::InternalState)?;
        }
        drop(state);

        let sampled = self.probe.sample().and_then(|snapshot| {
            validate_snapshot(&snapshot)?;
            Ok(snapshot)
        });
        let mut state = self.state.lock().map_err(|_| AiError::InternalState)?;
        state.sampling = false;
        let result = match sampled {
            Ok(snapshot) => {
                state.samples = state.samples.saturating_add(1);
                update_hysteresis(&mut state, &snapshot, self.config);
                state.cached = Some((now_ms, snapshot.clone()));
                state.last_failure = None;
                Ok((snapshot, state.pressure_limited))
            }
            Err(error) => {
                state.sample_failures = state.sample_failures.saturating_add(1);
                state.last_failure = Some((now_ms, error.clone()));
                Err(error)
            }
        };
        self.sampled.notify_all();
        result
    }
}

fn validate_config(
    config: ResourceGovernorConfig,
    contribution: AiContributionPolicy,
) -> AiResult<()> {
    if config.sampling_interval.is_zero()
        || config.hysteresis_samples == 0
        || config.max_workers == 0
        || config.max_concurrent_jobs == 0
        || config.pressure_queue_depth == 0
    {
        return Err(AiError::InvalidInput("resource governor config"));
    }
    if contribution.max_cpu_percent > 100 || contribution.max_gpu_percent > 100 {
        return Err(AiError::InvalidInput("contribution percentage"));
    }
    Ok(())
}

fn validate_snapshot(snapshot: &ResourceSnapshot) -> AiResult<()> {
    if snapshot.cpu_load_percent.is_some_and(|value| value > 100)
        || snapshot
            .process_cpu_percent
            .is_some_and(|value| value > 100)
        || snapshot
            .memory_pressure_percent
            .is_some_and(|value| value > 100)
        || snapshot.devices.len() > 64
        || snapshot.failures.len() > 16
        || snapshot.devices.iter().any(|device| {
            device.utilization_percent.is_some_and(|value| value > 100)
                || device.capabilities.compatible_apis.len() > 16
                || device
                    .available_memory_bytes
                    .zip(device.total_memory_bytes)
                    .is_some_and(|(available, total)| available > total)
                || device
                    .used_memory_bytes
                    .zip(device.total_memory_bytes)
                    .is_some_and(|(used, total)| used > total)
        })
        || snapshot
            .available_memory_bytes
            .zip(snapshot.total_memory_bytes)
            .is_some_and(|(available, total)| available > total)
    {
        return Err(AiError::InvalidInput("resource utilization"));
    }
    Ok(())
}

fn update_hysteresis(
    state: &mut GovernorState,
    snapshot: &ResourceSnapshot,
    config: ResourceGovernorConfig,
) {
    let pressure = snapshot.cpu_load_percent.is_some_and(|value| value >= 95)
        || snapshot
            .memory_pressure_percent
            .is_some_and(|value| value >= 20)
        || snapshot.devices.iter().any(device_pressure)
        || snapshot.queue_depth >= config.pressure_queue_depth
        || matches!(
            snapshot.thermal_pressure,
            ThermalPressure::Serious | ThermalPressure::Critical
        )
        || low_memory(snapshot);
    if pressure {
        state.pressure_samples = state.pressure_samples.saturating_add(1);
        state.recovery_samples = 0;
        if state.pressure_samples >= config.hysteresis_samples {
            state.pressure_limited = true;
        }
    } else {
        state.recovery_samples = state.recovery_samples.saturating_add(1);
        state.pressure_samples = 0;
        if state.recovery_samples >= config.hysteresis_samples {
            state.pressure_limited = false;
        }
    }
}

fn device_pressure(device: &crate::DeviceSnapshot) -> bool {
    !device.healthy
        || device.utilization_percent.is_some_and(|value| value >= 95)
        || device
            .available_memory_bytes
            .zip(device.total_memory_bytes)
            .is_some_and(|(available, total)| total > 0 && available < total / 20)
}

fn low_memory(snapshot: &ResourceSnapshot) -> bool {
    match (snapshot.total_memory_bytes, snapshot.available_memory_bytes) {
        (Some(total), Some(available)) => available < total / 20,
        _ => false,
    }
}
