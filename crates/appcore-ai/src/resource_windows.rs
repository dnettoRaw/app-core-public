// =============================================================================
//        #######
//     ###       ###     F: resource_windows.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/22 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/22 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

#[cfg(feature = "accelerator-nvidia")]
use crate::AcceleratorProbe;
use crate::{
    AiResult, DeviceApi, DeviceCapabilities, DeviceClass, DeviceId, DeviceKind, DeviceMemoryKind,
    DeviceSnapshot, HardwareProbe, ResourceProbeComponent, ResourceProbeFailure,
    ResourceProbeFailureKind, ResourceProbeStatus, ResourceSnapshot, ThermalPressure,
};
use std::mem::{size_of, zeroed};
use std::ptr;
use std::sync::Mutex;
use std::time::Instant;
use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::System::SystemInformation::{
    GetLogicalProcessorInformationEx, GlobalMemoryStatusEx, RelationProcessorCore, MEMORYSTATUSEX,
    SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes, GetSystemTimes};

#[derive(Clone, Copy, Debug)]
struct CpuCounters {
    total: u64,
    idle: u64,
    process: u64,
    observed_at: Instant,
}

#[derive(Debug)]
pub(crate) struct WindowsHardwareProbe {
    logical_cpus: Option<usize>,
    physical_cpus: Option<usize>,
    previous_cpu: Mutex<Option<CpuCounters>>,
    #[cfg(feature = "accelerator-nvidia")]
    nvidia: Option<crate::accelerator_nvidia::NvidiaAcceleratorProbe>,
}

impl WindowsHardwareProbe {
    pub(crate) fn new() -> Self {
        Self {
            logical_cpus: std::thread::available_parallelism().ok().map(usize::from),
            physical_cpus: physical_core_count(),
            previous_cpu: Mutex::new(None),
            #[cfg(feature = "accelerator-nvidia")]
            nvidia: crate::accelerator_nvidia::NvidiaAcceleratorProbe::new(),
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
                    .as_nanos(),
            )
            .unwrap_or(u64::MAX)
                / 100;
            let machine = elapsed.saturating_mul(
                u64::try_from(self.logical_cpus.unwrap_or(1).max(1)).unwrap_or(u64::MAX),
            );
            let process_delta = current.process.saturating_sub(old.process);
            let process = (machine > 0).then(|| percent(process_delta, machine));
            (cpu, process)
        });
        *previous = Some(current);
        percentages.unwrap_or((None, None))
    }
}

impl HardwareProbe for WindowsHardwareProbe {
    fn sample(&self) -> AiResult<ResourceSnapshot> {
        let mut failures = Vec::new();
        let (cpu_load_percent, process_cpu_percent) = match cpu_counters() {
            Some(counters) => self.cpu_percentages(counters),
            None => {
                failures.push(failure(ResourceProbeComponent::Cpu));
                (None, None)
            }
        };
        let memory = memory_snapshot();
        let (total_memory_bytes, available_memory_bytes, used_memory_bytes) = match memory {
            Some((total, available)) => (
                Some(total),
                Some(available),
                Some(total.saturating_sub(available)),
            ),
            None => {
                failures.push(failure(ResourceProbeComponent::Memory));
                (None, None, None)
            }
        };
        let mut devices = Vec::new();
        if self.logical_cpus.is_some() {
            devices.push(cpu_device(
                total_memory_bytes,
                available_memory_bytes,
                used_memory_bytes,
                cpu_load_percent,
            )?);
        }
        #[cfg(feature = "accelerator-nvidia")]
        if let Some(nvidia) = &self.nvidia {
            let sample = nvidia.sample_accelerators();
            devices.extend(sample.devices);
            failures.extend(sample.failures);
        }
        Ok(ResourceSnapshot {
            logical_cpus: self.logical_cpus,
            physical_cpus: self.physical_cpus,
            cpu_load_percent,
            process_cpu_percent,
            total_memory_bytes,
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

fn memory_snapshot() -> Option<(u64, u64)> {
    let mut memory = MEMORYSTATUSEX {
        dwLength: u32::try_from(size_of::<MEMORYSTATUSEX>()).ok()?,
        ..MEMORYSTATUSEX::default()
    };
    // SAFETY: `memory` is initialized with the required size and is a valid
    // writable output buffer. The API is read-only.
    (unsafe { GlobalMemoryStatusEx(ptr::addr_of_mut!(memory)) } != 0)
        .then_some((memory.ullTotalPhys, memory.ullAvailPhys))
}

fn cpu_counters() -> Option<CpuCounters> {
    // SAFETY: FILETIME is an output-only POD for these Win32 calls.
    let mut idle: FILETIME = unsafe { zeroed() };
    // SAFETY: FILETIME is an output-only POD for these Win32 calls.
    let mut kernel: FILETIME = unsafe { zeroed() };
    // SAFETY: FILETIME is an output-only POD for these Win32 calls.
    let mut user: FILETIME = unsafe { zeroed() };
    // SAFETY: all output pointers are valid and the call only reads counters.
    if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 {
        return None;
    }
    // SAFETY: each FILETIME is a writable output object.
    let mut creation: FILETIME = unsafe { zeroed() };
    // SAFETY: each FILETIME is a writable output object.
    let mut exit: FILETIME = unsafe { zeroed() };
    // SAFETY: each FILETIME is a writable output object.
    let mut process_kernel: FILETIME = unsafe { zeroed() };
    // SAFETY: each FILETIME is a writable output object.
    let mut process_user: FILETIME = unsafe { zeroed() };
    // SAFETY: the pseudo-handle is valid for the lifetime of the process and
    // all output pointers remain writable for the call.
    if unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut process_kernel,
            &mut process_user,
        )
    } == 0
    {
        return None;
    }
    Some(CpuCounters {
        total: filetime(kernel).saturating_add(filetime(user)),
        idle: filetime(idle),
        process: filetime(process_kernel).saturating_add(filetime(process_user)),
        observed_at: Instant::now(),
    })
}

fn physical_core_count() -> Option<usize> {
    let mut bytes = 0u32;
    // SAFETY: a null buffer is the documented size-query form.
    unsafe { GetLogicalProcessorInformationEx(RelationProcessorCore, ptr::null_mut(), &mut bytes) };
    if bytes == 0 || usize::try_from(bytes).ok()? > 16 * 1024 * 1024 {
        return None;
    }
    let word = size_of::<usize>();
    let words = usize::try_from(bytes).ok()?.div_ceil(word);
    let mut buffer = vec![0usize; words];
    // SAFETY: the aligned allocation has at least `bytes` writable bytes and
    // `bytes` is updated by the API to the initialized region length.
    if unsafe {
        GetLogicalProcessorInformationEx(
            RelationProcessorCore,
            buffer.as_mut_ptr().cast(),
            &mut bytes,
        )
    } == 0
    {
        return None;
    }
    let mut offset = 0usize;
    let mut cores = 0usize;
    let initialized = usize::try_from(bytes).ok()?;
    while offset < initialized {
        let remaining = initialized.saturating_sub(offset);
        if remaining < size_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>() {
            return None;
        }
        // SAFETY: bounds above contain a complete header; read_unaligned avoids
        // assuming the variable-length record's internal alignment.
        let record = unsafe {
            buffer
                .as_ptr()
                .cast::<u8>()
                .add(offset)
                .cast::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>()
                .read_unaligned()
        };
        let size = usize::try_from(record.Size).ok()?;
        if size < size_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>() || size > remaining {
            return None;
        }
        if record.Relationship == RelationProcessorCore {
            cores = cores.saturating_add(1);
        }
        offset = offset.saturating_add(size);
    }
    (cores > 0).then_some(cores)
}

fn filetime(value: FILETIME) -> u64 {
    (u64::from(value.dwHighDateTime) << 32) | u64::from(value.dwLowDateTime)
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
