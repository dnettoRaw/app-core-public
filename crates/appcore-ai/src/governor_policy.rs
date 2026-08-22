// =============================================================================
//        #######
//     ###       ###     F: governor_policy.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/22 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/22 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{
    AdmissionDecision, AdmissionReason, AiContributionPolicy, AiError, AiResourceLimits,
    AiResourceMode, AiResult, DeviceKind, DeviceMemoryKind, DeviceSnapshot, ResourceBudget,
    ResourceEstimate, ResourceGovernorConfig, ResourceSnapshot, ThermalPressure,
};
use std::time::Duration;

pub(crate) fn validate_estimate(estimate: ResourceEstimate) -> AiResult<()> {
    if estimate.cpu_percent > 100 || estimate.gpu_percent > 100 || estimate.workers == 0 {
        return Err(AiError::InvalidInput("resource estimate"));
    }
    Ok(())
}

pub(crate) fn local_budget(
    snapshot: &ResourceSnapshot,
    config: ResourceGovernorConfig,
    mode: AiResourceMode,
    pressure_limited: bool,
) -> ResourceBudget {
    let (cpu_percent, gpu_percent, headroom_percent, custom) = mode_policy(mode);
    let logical = snapshot.logical_cpus.unwrap_or(1).max(1);
    let mut workers = logical
        .saturating_mul(usize::from(cpu_percent))
        .div_ceil(100)
        .clamp(1, config.max_workers);
    let mut concurrent_jobs = mode_concurrency(mode, workers, config.max_concurrent_jobs);
    let mut memory_bytes = available_after_headroom(snapshot, headroom_percent, config);
    let mut vram_bytes = available_vram(snapshot, headroom_percent);
    if let Some(limits) = custom {
        workers = workers.min(limits.max_workers.max(1));
        concurrent_jobs = concurrent_jobs.min(limits.max_concurrent_jobs.max(1));
        memory_bytes = Some(
            memory_bytes
                .unwrap_or(limits.max_memory_bytes)
                .min(limits.max_memory_bytes),
        );
        vram_bytes = Some(
            vram_bytes
                .unwrap_or(limits.max_vram_bytes)
                .min(limits.max_vram_bytes),
        );
    }
    if pressure_limited {
        workers = workers.div_ceil(2).max(1);
        concurrent_jobs = concurrent_jobs.div_ceil(2).max(1);
        memory_bytes = memory_bytes.map(|value| value / 2);
        vram_bytes = vram_bytes.map(|value| value / 2);
    }
    ResourceBudget {
        cpu_percent: limited_percent(cpu_percent, pressure_limited),
        gpu_percent: limited_percent(gpu_percent, pressure_limited),
        memory_bytes,
        vram_bytes,
        storage_bytes: 0,
        workers,
        concurrent_jobs,
        pressure_limited,
    }
}

fn limited_percent(value: u8, pressure_limited: bool) -> u8 {
    if pressure_limited {
        value.div_ceil(2)
    } else {
        value
    }
}

fn mode_policy(mode: AiResourceMode) -> (u8, u8, u8, Option<AiResourceLimits>) {
    match mode {
        AiResourceMode::Eco => (40, 40, 30, None),
        AiResourceMode::Balanced => (70, 70, 20, None),
        AiResourceMode::Performance => (90, 90, 10, None),
        AiResourceMode::Unrestricted => (100, 100, 0, None),
        AiResourceMode::Custom(limits) => {
            (limits.max_cpu_percent.clamp(1, 100), 100, 0, Some(limits))
        }
    }
}

fn mode_concurrency(mode: AiResourceMode, workers: usize, maximum: usize) -> usize {
    let requested = match mode {
        AiResourceMode::Eco => 1,
        AiResourceMode::Balanced => workers.div_ceil(2),
        AiResourceMode::Performance | AiResourceMode::Unrestricted => workers,
        AiResourceMode::Custom(limits) => limits.max_concurrent_jobs,
    };
    requested.clamp(1, maximum)
}

fn available_after_headroom(
    snapshot: &ResourceSnapshot,
    headroom_percent: u8,
    config: ResourceGovernorConfig,
) -> Option<u64> {
    let available = snapshot.available_memory_bytes?;
    let percentage = snapshot
        .total_memory_bytes
        .unwrap_or(available)
        .saturating_mul(u64::from(headroom_percent))
        / 100;
    Some(
        available
            .saturating_sub(percentage)
            .saturating_sub(config.reserved_memory_bytes),
    )
}

fn available_vram(snapshot: &ResourceSnapshot, headroom_percent: u8) -> Option<u64> {
    snapshot
        .devices
        .iter()
        .filter(|device| {
            device.kind == DeviceKind::Gpu
                && device.healthy
                && device.capabilities.memory_kind == DeviceMemoryKind::Dedicated
        })
        .filter_map(|device| available_device_memory(device, headroom_percent))
        .max()
}

pub(crate) fn device_vram(device: &DeviceSnapshot, mode: AiResourceMode) -> Option<u64> {
    if device.capabilities.memory_kind != DeviceMemoryKind::Dedicated {
        return None;
    }
    let (_, _, headroom_percent, _) = mode_policy(mode);
    available_device_memory(device, headroom_percent)
}

fn available_device_memory(device: &DeviceSnapshot, headroom_percent: u8) -> Option<u64> {
    let available = device.available_memory_bytes?;
    let reserved = device
        .total_memory_bytes
        .unwrap_or(available)
        .saturating_mul(u64::from(headroom_percent))
        / 100;
    Some(available.saturating_sub(reserved))
}

pub(crate) fn contribution_budget(
    local: ResourceBudget,
    contribution: AiContributionPolicy,
) -> ResourceBudget {
    let compute = contribution.contribute_compute;
    ResourceBudget {
        cpu_percent: contribution_percent(compute, local.cpu_percent, contribution.max_cpu_percent),
        gpu_percent: contribution_percent(compute, local.gpu_percent, contribution.max_gpu_percent),
        memory_bytes: contribution_memory(local, contribution),
        vram_bytes: if compute {
            Some(
                local
                    .vram_bytes
                    .unwrap_or(contribution.max_vram_bytes)
                    .min(contribution.max_vram_bytes),
            )
        } else {
            Some(0)
        },
        storage_bytes: if contribution.contribute_storage {
            contribution.max_storage_bytes
        } else {
            0
        },
        workers: if compute {
            local.workers.min(contribution.max_workers)
        } else {
            0
        },
        concurrent_jobs: if compute {
            local.concurrent_jobs.min(contribution.max_concurrent_jobs)
        } else {
            0
        },
        pressure_limited: local.pressure_limited,
    }
}

fn contribution_percent(enabled: bool, local: u8, maximum: u8) -> u8 {
    if enabled {
        local.min(maximum)
    } else {
        0
    }
}

fn contribution_memory(local: ResourceBudget, contribution: AiContributionPolicy) -> Option<u64> {
    if contribution.contribute_compute || contribution.contribute_storage {
        Some(
            local
                .memory_bytes
                .unwrap_or(contribution.max_memory_bytes)
                .min(contribution.max_memory_bytes),
        )
    } else {
        Some(0)
    }
}

pub(crate) fn admission(
    snapshot: &ResourceSnapshot,
    budget: ResourceBudget,
    estimate: ResourceEstimate,
    retry_after: Duration,
    memory_kind: Option<DeviceMemoryKind>,
) -> AdmissionDecision {
    if matches!(snapshot.thermal_pressure, ThermalPressure::Critical) {
        return AdmissionDecision::Defer {
            reason: AdmissionReason::ThermalPressure,
            retry_after,
        };
    }
    if snapshot.active_jobs >= budget.concurrent_jobs {
        return AdmissionDecision::Defer {
            reason: AdmissionReason::ConcurrentJobLimit,
            retry_after,
        };
    }
    let required_memory = if memory_kind == Some(DeviceMemoryKind::Unified) {
        estimate.memory_bytes.saturating_add(estimate.vram_bytes)
    } else {
        estimate.memory_bytes
    };
    if let Some(decision) = memory_admission(
        required_memory,
        budget.memory_bytes,
        AdmissionReason::MemoryPressure,
        retry_after,
    ) {
        return decision;
    }
    if memory_kind != Some(DeviceMemoryKind::Unified) {
        if let Some(decision) = memory_admission(
            estimate.vram_bytes,
            budget.vram_bytes,
            AdmissionReason::VramPressure,
            retry_after,
        ) {
            return decision;
        }
    }
    if estimate.cpu_percent > budget.cpu_percent {
        return AdmissionDecision::Reject {
            reason: AdmissionReason::CpuPressure,
        };
    }
    if estimate.gpu_percent > budget.gpu_percent {
        return AdmissionDecision::Reject {
            reason: AdmissionReason::GpuPressure,
        };
    }
    if estimate.workers > budget.workers {
        return AdmissionDecision::Reject {
            reason: AdmissionReason::WorkerLimit,
        };
    }
    AdmissionDecision::Admit { budget }
}

fn memory_admission(
    required: u64,
    available: Option<u64>,
    reason: AdmissionReason,
    retry_after: Duration,
) -> Option<AdmissionDecision> {
    if required > 0 && available.is_none() {
        return Some(AdmissionDecision::Defer {
            reason: AdmissionReason::UnknownCapacity,
            retry_after,
        });
    }
    available
        .is_some_and(|value| required > value)
        .then_some(AdmissionDecision::Defer {
            reason,
            retry_after,
        })
}

pub(crate) fn duration_ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
