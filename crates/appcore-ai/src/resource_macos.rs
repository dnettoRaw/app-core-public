// =============================================================================
//        #######
//     ###       ###     F: resource_macos.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/22 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/22 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{
    AiResult, DeviceApi, DeviceCapabilities, DeviceClass, DeviceId, DeviceKind, DeviceMemoryKind,
    DeviceSnapshot, HardwareProbe, ResourceProbeComponent, ResourceProbeFailure,
    ResourceProbeFailureKind, ResourceProbeStatus, ResourceSnapshot, ThermalPressure,
};
use std::ffi::CString;
use std::mem::{size_of, zeroed};
use std::ptr;
use std::sync::Mutex;
use std::time::Instant;

#[derive(Clone, Copy, Debug)]
struct CpuCounters {
    total: u64,
    idle: u64,
    process_micros: u64,
    observed_at: Instant,
}

#[derive(Debug)]
pub(crate) struct MacOsHardwareProbe {
    logical_cpus: Option<usize>,
    physical_cpus: Option<usize>,
    total_memory_bytes: Option<u64>,
    page_size: Option<u64>,
    previous_cpu: Mutex<Option<CpuCounters>>,
}

impl MacOsHardwareProbe {
    pub(crate) fn new() -> Self {
        Self {
            logical_cpus: sysctl_u64("hw.logicalcpu").and_then(|value| usize::try_from(value).ok()),
            physical_cpus: sysctl_u64("hw.physicalcpu")
                .and_then(|value| usize::try_from(value).ok()),
            total_memory_bytes: sysctl_u64("hw.memsize"),
            page_size: page_size(),
            previous_cpu: Mutex::new(None),
        }
    }

    fn cpu_percentages(&self, current: CpuCounters) -> (Option<u8>, Option<u8>) {
        let Ok(mut previous) = self.previous_cpu.lock() else {
            return (None, None);
        };
        let percentages = previous.map(|old| {
            let total = current.total.saturating_sub(old.total);
            let idle = current.idle.saturating_sub(old.idle);
            let cpu = (total > 0).then(|| percent(total.saturating_sub(idle), total));
            let elapsed = u64::try_from(
                current
                    .observed_at
                    .duration_since(old.observed_at)
                    .as_micros(),
            )
            .unwrap_or(u64::MAX);
            let process_delta = current.process_micros.saturating_sub(old.process_micros);
            let machine = elapsed.saturating_mul(
                u64::try_from(self.logical_cpus.unwrap_or(1).max(1)).unwrap_or(u64::MAX),
            );
            let process = (machine > 0).then(|| percent(process_delta, machine));
            (cpu, process)
        });
        *previous = Some(current);
        percentages.unwrap_or((None, None))
    }
}

impl HardwareProbe for MacOsHardwareProbe {
    fn sample(&self) -> AiResult<ResourceSnapshot> {
        let mut failures = Vec::new();
        let (cpu_load_percent, process_cpu_percent) = match cpu_counters() {
            Some(counters) => self.cpu_percentages(counters),
            None => {
                failures.push(failure(ResourceProbeComponent::Cpu));
                (None, None)
            }
        };
        let available_memory_bytes = match self.page_size.and_then(available_memory) {
            Some(value) => Some(value.min(self.total_memory_bytes.unwrap_or(value))),
            None => {
                failures.push(failure(ResourceProbeComponent::Memory));
                None
            }
        };
        let used_memory_bytes = self
            .total_memory_bytes
            .zip(available_memory_bytes)
            .map(|(total, available)| total.saturating_sub(available));
        let mut devices = Vec::new();
        if self.logical_cpus.is_some() {
            devices.push(cpu_device(
                self.total_memory_bytes,
                available_memory_bytes,
                used_memory_bytes,
                cpu_load_percent,
            )?);
        }
        #[cfg(target_arch = "aarch64")]
        devices.push(apple_gpu(
            self.total_memory_bytes,
            available_memory_bytes,
            used_memory_bytes,
        )?);
        Ok(ResourceSnapshot {
            logical_cpus: self.logical_cpus,
            physical_cpus: self.physical_cpus,
            cpu_load_percent,
            process_cpu_percent,
            total_memory_bytes: self.total_memory_bytes,
            available_memory_bytes,
            used_memory_bytes,
            memory_pressure_percent: None,
            devices,
            thermal_pressure: ThermalPressure::Unknown,
            status: if failures.is_empty() {
                ResourceProbeStatus::Healthy
            } else {
                ResourceProbeStatus::Degraded
            },
            failures,
            ..ResourceSnapshot::default()
        })
    }
}

fn cpu_device(
    total: Option<u64>,
    available: Option<u64>,
    used: Option<u64>,
    utilization: Option<u8>,
) -> AiResult<DeviceSnapshot> {
    Ok(DeviceSnapshot {
        id: DeviceId::new("local/cpu")?,
        kind: DeviceKind::Cpu,
        capabilities: DeviceCapabilities {
            class: DeviceClass::Cpu,
            memory_kind: DeviceMemoryKind::Unified,
            compatible_apis: vec![DeviceApi::Cpu],
        },
        total_memory_bytes: total,
        available_memory_bytes: available,
        used_memory_bytes: used,
        utilization_percent: utilization,
        healthy: true,
    })
}

#[cfg(target_arch = "aarch64")]
fn apple_gpu(
    total: Option<u64>,
    available: Option<u64>,
    used: Option<u64>,
) -> AiResult<DeviceSnapshot> {
    Ok(DeviceSnapshot {
        id: DeviceId::new("local/gpu/apple-silicon")?,
        kind: DeviceKind::Gpu,
        capabilities: DeviceCapabilities {
            class: DeviceClass::IntegratedGpu,
            memory_kind: DeviceMemoryKind::Unified,
            compatible_apis: vec![DeviceApi::Metal],
        },
        total_memory_bytes: total,
        available_memory_bytes: available,
        used_memory_bytes: used,
        utilization_percent: None,
        healthy: true,
    })
}

fn sysctl_u64(name: &str) -> Option<u64> {
    let name = CString::new(name).ok()?;
    let mut value = 0u64;
    let mut length = size_of::<u64>();
    // SAFETY: `name` is NUL-terminated and both output pointers refer to valid,
    // writable objects for the supplied length. This is a read-only sysctl.
    let result = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            ptr::addr_of_mut!(value).cast(),
            ptr::addr_of_mut!(length),
            ptr::null_mut(),
            0,
        )
    };
    (result == 0 && (length == size_of::<u32>() || length == size_of::<u64>())).then_some(value)
}

fn page_size() -> Option<u64> {
    // SAFETY: `_SC_PAGESIZE` has no pointer arguments and does not mutate state.
    let value = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    (value > 0).then(|| u64::try_from(value).ok()).flatten()
}

#[allow(deprecated)] // libc exposes the stable Mach ABI without another dependency.
fn available_memory(page_size: u64) -> Option<u64> {
    // SAFETY: the zeroed C struct is valid for this output-only Mach call, and
    // the count exactly matches `vm_statistics64_data_t`.
    let mut statistics: libc::vm_statistics64_data_t = unsafe { zeroed() };
    let mut count = libc::HOST_VM_INFO64_COUNT;
    // SAFETY: all pointers are valid for `count`; `mach_host_self` returns a
    // send right owned by the task and the call is read-only.
    let result = unsafe {
        libc::host_statistics64(
            libc::mach_host_self(),
            libc::HOST_VM_INFO64,
            ptr::addr_of_mut!(statistics).cast(),
            ptr::addr_of_mut!(count),
        )
    };
    if result != libc::KERN_SUCCESS {
        return None;
    }
    let pages =
        u64::from(statistics.free_count).saturating_add(u64::from(statistics.inactive_count));
    pages.checked_mul(page_size)
}

#[allow(deprecated)] // libc exposes the stable Mach ABI without another dependency.
fn cpu_counters() -> Option<CpuCounters> {
    // SAFETY: `host_cpu_load_info_data_t` is an output POD for Mach.
    let mut cpu: libc::host_cpu_load_info_data_t = unsafe { zeroed() };
    let mut count = libc::HOST_CPU_LOAD_INFO_COUNT;
    // SAFETY: pointers and element count match the output structure.
    let result = unsafe {
        libc::host_statistics(
            libc::mach_host_self(),
            libc::HOST_CPU_LOAD_INFO,
            ptr::addr_of_mut!(cpu).cast(),
            ptr::addr_of_mut!(count),
        )
    };
    if result != libc::KERN_SUCCESS {
        return None;
    }
    // SAFETY: zeroed `rusage` is the required output buffer for this read-only call.
    let mut usage: libc::rusage = unsafe { zeroed() };
    // SAFETY: `usage` is valid and writable for the duration of the call.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, ptr::addr_of_mut!(usage)) } != 0 {
        return None;
    }
    let total = cpu.cpu_ticks.iter().map(|value| u64::from(*value)).sum();
    let idle = u64::from(cpu.cpu_ticks[libc::CPU_STATE_IDLE as usize]);
    Some(CpuCounters {
        total,
        idle,
        process_micros: timeval_micros(usage.ru_utime)
            .saturating_add(timeval_micros(usage.ru_stime)),
        observed_at: Instant::now(),
    })
}

fn timeval_micros(value: libc::timeval) -> u64 {
    u64::try_from(value.tv_sec)
        .unwrap_or_default()
        .saturating_mul(1_000_000)
        .saturating_add(u64::try_from(value.tv_usec).unwrap_or_default())
}

fn percent(value: u64, total: u64) -> u8 {
    u8::try_from(value.saturating_mul(100).div_ceil(total).min(100)).unwrap_or(100)
}

fn failure(component: ResourceProbeComponent) -> ResourceProbeFailure {
    ResourceProbeFailure {
        component,
        kind: ResourceProbeFailureKind::Unavailable,
    }
}
