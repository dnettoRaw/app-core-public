// =============================================================================
//        #######
//     ###       ###     F: resource_linux.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/22 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/22 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{
    AcceleratorProbe, AcceleratorSample, AiResult, DeviceApi, DeviceCapabilities, DeviceClass,
    DeviceId, DeviceKind, DeviceMemoryKind, DeviceSnapshot, HardwareProbe, ResourceProbeComponent,
    ResourceProbeFailure, ResourceProbeFailureKind, ResourceProbeStatus, ResourceSnapshot,
    ThermalPressure,
};
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const MAX_PROC_BYTES: u64 = 128 * 1024;
const MAX_DRM_DEVICES: usize = 64;
const DEVICE_REDISCOVERY: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug)]
struct CpuCounters {
    total: u64,
    idle: u64,
    process: u64,
}

#[derive(Debug)]
pub(crate) struct LinuxHardwareProbe {
    logical_cpus: Option<usize>,
    physical_cpus: Option<usize>,
    previous_cpu: Mutex<Option<CpuCounters>>,
    accelerators: LinuxAcceleratorProbe,
    #[cfg(feature = "accelerator-nvidia")]
    nvidia: Option<crate::accelerator_nvidia::NvidiaAcceleratorProbe>,
}

impl LinuxHardwareProbe {
    pub(crate) fn new() -> Self {
        #[cfg(feature = "accelerator-nvidia")]
        let nvidia = crate::accelerator_nvidia::NvidiaAcceleratorProbe::new();
        #[cfg(feature = "accelerator-nvidia")]
        let prefer_nvml = nvidia.is_some();
        #[cfg(not(feature = "accelerator-nvidia"))]
        let prefer_nvml = false;
        Self {
            logical_cpus: std::thread::available_parallelism().ok().map(usize::from),
            physical_cpus: physical_core_count(),
            previous_cpu: Mutex::new(None),
            accelerators: LinuxAcceleratorProbe::new(prefer_nvml),
            #[cfg(feature = "accelerator-nvidia")]
            nvidia,
        }
    }
}

impl HardwareProbe for LinuxHardwareProbe {
    fn sample(&self) -> AiResult<ResourceSnapshot> {
        let mut failures = Vec::new();
        let counters = cpu_counters().map_err(|error| failure_kind(&error));
        let (cpu_load_percent, process_cpu_percent) = match counters {
            Ok(current) => self.cpu_percentages(current),
            Err(kind) => {
                failures.push(failure(ResourceProbeComponent::Cpu, kind));
                (None, None)
            }
        };
        let memory = memory_snapshot();
        let (total_memory_bytes, available_memory_bytes) = match memory {
            Ok(values) => values,
            Err(error) => {
                failures.push(failure(
                    ResourceProbeComponent::Memory,
                    failure_kind(&error),
                ));
                (None, None)
            }
        };
        #[cfg(feature = "accelerator-nvidia")]
        let mut accelerator_sample = self.accelerators.sample_accelerators();
        #[cfg(not(feature = "accelerator-nvidia"))]
        let accelerator_sample = self.accelerators.sample_accelerators();
        #[cfg(feature = "accelerator-nvidia")]
        if let Some(nvidia) = &self.nvidia {
            let sample = nvidia.sample_accelerators();
            accelerator_sample.devices.extend(sample.devices);
            accelerator_sample.failures.extend(sample.failures);
        }
        failures.extend(accelerator_sample.failures.into_iter().take(16));
        let mut devices = cpu_device(
            self.logical_cpus,
            total_memory_bytes,
            available_memory_bytes,
            cpu_load_percent,
        )?;
        devices.extend(accelerator_sample.devices.into_iter().take(MAX_DRM_DEVICES));
        let used_memory_bytes = total_memory_bytes
            .zip(available_memory_bytes)
            .map(|(total, available)| total.saturating_sub(available));
        let memory_pressure_percent = pressure_average("/proc/pressure/memory");
        Ok(ResourceSnapshot {
            logical_cpus: self.logical_cpus,
            physical_cpus: self.physical_cpus,
            cpu_load_percent,
            process_cpu_percent,
            total_memory_bytes,
            available_memory_bytes,
            used_memory_bytes,
            memory_pressure_percent,
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

impl LinuxHardwareProbe {
    fn cpu_percentages(&self, current: CpuCounters) -> (Option<u8>, Option<u8>) {
        let Ok(mut previous) = self.previous_cpu.lock() else {
            return (None, None);
        };
        let percentages = previous.map(|old| {
            let total = current.total.saturating_sub(old.total);
            let idle = current.idle.saturating_sub(old.idle);
            let process = current.process.saturating_sub(old.process);
            if total == 0 {
                return (None, None);
            }
            (
                percent(total.saturating_sub(idle), total),
                percent(process, total),
            )
        });
        *previous = Some(current);
        percentages.unwrap_or((None, None))
    }
}

fn cpu_device(
    logical: Option<usize>,
    total: Option<u64>,
    available: Option<u64>,
    utilization: Option<u8>,
) -> AiResult<Vec<DeviceSnapshot>> {
    logical
        .map(|_| {
            Ok(vec![DeviceSnapshot {
                id: DeviceId::new("local/cpu")?,
                kind: DeviceKind::Cpu,
                capabilities: DeviceCapabilities {
                    class: DeviceClass::Cpu,
                    memory_kind: DeviceMemoryKind::Unified,
                    compatible_apis: vec![DeviceApi::Cpu],
                },
                total_memory_bytes: total,
                available_memory_bytes: available,
                used_memory_bytes: total.zip(available).map(|(t, a)| t.saturating_sub(a)),
                utilization_percent: utilization,
                healthy: true,
            }])
        })
        .unwrap_or_else(|| Ok(Vec::new()))
}

fn cpu_counters() -> io::Result<CpuCounters> {
    let stat = read_bounded(Path::new("/proc/stat"), MAX_PROC_BYTES)?;
    let cpu = stat
        .lines()
        .find(|line| line.starts_with("cpu "))
        .ok_or_else(invalid_data)?;
    let values = cpu
        .split_ascii_whitespace()
        .skip(1)
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| invalid_data())?;
    if values.len() < 4 {
        return Err(invalid_data());
    }
    let total = values.iter().copied().fold(0u64, u64::saturating_add);
    let idle = values[3].saturating_add(values.get(4).copied().unwrap_or_default());
    let process = process_ticks().unwrap_or_default();
    Ok(CpuCounters {
        total,
        idle,
        process,
    })
}

fn process_ticks() -> io::Result<u64> {
    let stat = read_bounded(Path::new("/proc/self/stat"), 16 * 1024)?;
    let suffix = stat.rsplit_once(')').ok_or_else(invalid_data)?.1;
    let fields = suffix.split_ascii_whitespace().collect::<Vec<_>>();
    let user = fields
        .get(11)
        .ok_or_else(invalid_data)?
        .parse::<u64>()
        .map_err(|_| invalid_data())?;
    let system = fields
        .get(12)
        .ok_or_else(invalid_data)?
        .parse::<u64>()
        .map_err(|_| invalid_data())?;
    Ok(user.saturating_add(system))
}

fn memory_snapshot() -> io::Result<(Option<u64>, Option<u64>)> {
    let contents = read_bounded(Path::new("/proc/meminfo"), MAX_PROC_BYTES)?;
    let mut total = None;
    let mut available = None;
    for line in contents.lines() {
        let mut fields = line.split_ascii_whitespace();
        match fields.next() {
            Some("MemTotal:") => total = kib_field(fields.next()),
            Some("MemAvailable:") => available = kib_field(fields.next()),
            _ => {}
        }
    }
    if total.is_none() || available.is_none() {
        return Err(invalid_data());
    }
    Ok((total, available))
}

fn kib_field(value: Option<&str>) -> Option<u64> {
    value?.parse::<u64>().ok()?.checked_mul(1024)
}

fn pressure_average(path: &str) -> Option<u8> {
    let contents = read_bounded(Path::new(path), 8 * 1024).ok()?;
    let some = contents.lines().find(|line| line.starts_with("some "))?;
    let average = some
        .split_ascii_whitespace()
        .find_map(|field| field.strip_prefix("avg10="))?
        .parse::<f64>()
        .ok()?;
    if average.is_finite() {
        Some(average.round().clamp(0.0, 100.0) as u8)
    } else {
        None
    }
}

fn physical_core_count() -> Option<usize> {
    let entries = fs::read_dir("/sys/devices/system/cpu").ok()?;
    let mut cores = BTreeSet::new();
    for entry in entries.flatten().take(1_024) {
        let name = entry.file_name();
        let name = name.to_str()?;
        let Some(suffix) = name.strip_prefix("cpu") else {
            continue;
        };
        if suffix.parse::<usize>().is_err() {
            continue;
        }
        let topology = entry.path().join("topology");
        let Some(package) = read_number(&topology.join("physical_package_id")) else {
            continue;
        };
        let Some(core) = read_number(&topology.join("core_id")) else {
            continue;
        };
        cores.insert((package, core));
    }
    (!cores.is_empty()).then_some(cores.len())
}

#[derive(Clone, Debug)]
struct LinuxDevice {
    card: String,
    vendor: u16,
    path: PathBuf,
}

#[derive(Debug)]
struct LinuxDiscovery {
    devices: Vec<LinuxDevice>,
    discovered_at: Instant,
}

#[derive(Debug)]
struct LinuxAcceleratorProbe {
    discovery: Mutex<LinuxDiscovery>,
    prefer_nvml: bool,
}

impl LinuxAcceleratorProbe {
    fn new(prefer_nvml: bool) -> Self {
        Self {
            discovery: Mutex::new(LinuxDiscovery {
                devices: discover_drm(),
                discovered_at: Instant::now(),
            }),
            prefer_nvml,
        }
    }
}

impl AcceleratorProbe for LinuxAcceleratorProbe {
    fn sample_accelerators(&self) -> AcceleratorSample {
        let Ok(mut discovery) = self.discovery.lock() else {
            return AcceleratorSample {
                failures: vec![failure(
                    ResourceProbeComponent::Accelerator,
                    ResourceProbeFailureKind::InvalidData,
                )],
                ..AcceleratorSample::default()
            };
        };
        if discovery.discovered_at.elapsed() >= DEVICE_REDISCOVERY {
            discovery.devices = discover_drm();
            discovery.discovered_at = Instant::now();
        }
        let mut sample = AcceleratorSample::default();
        for device in &discovery.devices {
            if self.prefer_nvml && device.vendor == 0x10de {
                continue;
            }
            match drm_snapshot(device) {
                Ok(snapshot) => sample.devices.push(snapshot),
                Err(_) => sample.failures.push(failure(
                    ResourceProbeComponent::Accelerator,
                    ResourceProbeFailureKind::InvalidData,
                )),
            }
        }
        sample
    }
}

fn discover_drm() -> Vec<LinuxDevice> {
    let Ok(entries) = fs::read_dir("/sys/class/drm") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let card = entry.file_name().to_str()?.to_owned();
            card.strip_prefix("card")?.parse::<usize>().ok()?;
            let path = entry.path().join("device");
            let vendor = read_hex(&path.join("vendor"))?;
            Some(LinuxDevice { card, vendor, path })
        })
        .take(MAX_DRM_DEVICES)
        .collect()
}

fn drm_snapshot(device: &LinuxDevice) -> AiResult<DeviceSnapshot> {
    let total = read_number(&device.path.join("mem_info_vram_total"));
    let used = read_number(&device.path.join("mem_info_vram_used"));
    let available = total
        .zip(used)
        .map(|(total, used)| total.saturating_sub(used));
    let utilization = read_number(&device.path.join("gpu_busy_percent"))
        .and_then(|value| u8::try_from(value.min(100)).ok());
    let memory_kind = if total.is_some_and(|value| value > 0) {
        DeviceMemoryKind::Dedicated
    } else {
        DeviceMemoryKind::Unknown
    };
    let compatible_apis = if device.vendor == 0x1002 && Path::new("/dev/kfd").exists() {
        vec![DeviceApi::Rocm]
    } else {
        Vec::new()
    };
    Ok(DeviceSnapshot {
        id: DeviceId::new(format!("local/gpu/linux/{}", device.card))?,
        kind: DeviceKind::Gpu,
        capabilities: DeviceCapabilities {
            class: if memory_kind == DeviceMemoryKind::Dedicated {
                DeviceClass::DiscreteGpu
            } else {
                DeviceClass::Unknown
            },
            memory_kind,
            compatible_apis,
        },
        total_memory_bytes: total,
        available_memory_bytes: available,
        used_memory_bytes: used,
        utilization_percent: utilization,
        healthy: device.path.exists(),
    })
}

fn read_bounded(path: &Path, maximum: u64) -> io::Result<String> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(maximum.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > maximum {
        return Err(invalid_data());
    }
    String::from_utf8(bytes).map_err(|_| invalid_data())
}

fn read_number(path: &Path) -> Option<u64> {
    read_bounded(path, 128).ok()?.trim().parse().ok()
}

fn read_hex(path: &Path) -> Option<u16> {
    u16::from_str_radix(
        read_bounded(path, 128)
            .ok()?
            .trim()
            .trim_start_matches("0x"),
        16,
    )
    .ok()
}

fn percent(value: u64, total: u64) -> Option<u8> {
    u8::try_from(value.saturating_mul(100).div_ceil(total).min(100)).ok()
}

fn failure(
    component: ResourceProbeComponent,
    kind: ResourceProbeFailureKind,
) -> ResourceProbeFailure {
    ResourceProbeFailure { component, kind }
}

fn failure_kind(error: &io::Error) -> ResourceProbeFailureKind {
    match error.kind() {
        io::ErrorKind::PermissionDenied => ResourceProbeFailureKind::PermissionDenied,
        io::ErrorKind::NotFound | io::ErrorKind::Unsupported => {
            ResourceProbeFailureKind::Unavailable
        }
        _ => ResourceProbeFailureKind::InvalidData,
    }
}

fn invalid_data() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid bounded hardware data")
}
