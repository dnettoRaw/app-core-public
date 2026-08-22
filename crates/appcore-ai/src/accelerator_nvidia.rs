// =============================================================================
//        #######
//     ###       ###     F: accelerator_nvidia.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/22 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/22 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{
    AcceleratorProbe, AcceleratorSample, DeviceApi, DeviceCapabilities, DeviceClass, DeviceId,
    DeviceKind, DeviceMemoryKind, DeviceSnapshot, ResourceProbeComponent, ResourceProbeFailure,
    ResourceProbeFailureKind,
};
use nvml_wrapper::Nvml;

/// Optional read-only NVIDIA NVML accelerator probe.
pub(crate) struct NvidiaAcceleratorProbe {
    nvml: Nvml,
}

impl std::fmt::Debug for NvidiaAcceleratorProbe {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NvidiaAcceleratorProbe")
            .finish_non_exhaustive()
    }
}

impl NvidiaAcceleratorProbe {
    pub(crate) fn new() -> Option<Self> {
        Nvml::init().ok().map(|nvml| Self { nvml })
    }
}

impl AcceleratorProbe for NvidiaAcceleratorProbe {
    fn sample_accelerators(&self) -> AcceleratorSample {
        let Ok(count) = self.nvml.device_count() else {
            return failed_sample();
        };
        let mut sample = AcceleratorSample::default();
        for index in 0..count.min(64) {
            match self.device(index) {
                Some(device) => sample.devices.push(device),
                None => sample.failures.push(driver_failure()),
            }
        }
        sample
    }
}

impl NvidiaAcceleratorProbe {
    fn device(&self, index: u32) -> Option<DeviceSnapshot> {
        let device = self.nvml.device_by_index(index).ok()?;
        let memory = device.memory_info().ok();
        let utilization = device
            .utilization_rates()
            .ok()
            .and_then(|rates| u8::try_from(rates.gpu.min(100)).ok());
        if memory.is_none() && utilization.is_none() {
            return None;
        }
        Some(DeviceSnapshot {
            id: DeviceId::new(format!("local/gpu/nvidia/{index}")).ok()?,
            kind: DeviceKind::Gpu,
            capabilities: DeviceCapabilities {
                class: DeviceClass::DiscreteGpu,
                memory_kind: DeviceMemoryKind::Dedicated,
                compatible_apis: vec![DeviceApi::Cuda],
            },
            total_memory_bytes: memory.as_ref().map(|value| value.total),
            available_memory_bytes: memory.as_ref().map(|value| value.free),
            used_memory_bytes: memory.as_ref().map(|value| value.used),
            utilization_percent: utilization,
            healthy: true,
        })
    }
}

fn failed_sample() -> AcceleratorSample {
    AcceleratorSample {
        failures: vec![driver_failure()],
        ..AcceleratorSample::default()
    }
}

fn driver_failure() -> ResourceProbeFailure {
    ResourceProbeFailure {
        component: ResourceProbeComponent::Accelerator,
        kind: ResourceProbeFailureKind::Driver,
    }
}
