// =============================================================================
//        #######
//     ###       ###     F: resource_fallback.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/22 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/22 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{
    AiResult, DeviceApi, DeviceCapabilities, DeviceClass, DeviceId, DeviceKind, DeviceMemoryKind,
    DeviceSnapshot, HardwareProbe, ResourceProbeStatus, ResourceSnapshot, ThermalPressure,
};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct FallbackHardwareProbe;

impl FallbackHardwareProbe {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl HardwareProbe for FallbackHardwareProbe {
    fn sample(&self) -> AiResult<ResourceSnapshot> {
        let logical_cpus = std::thread::available_parallelism().ok().map(usize::from);
        let devices = logical_cpus
            .map(|_| {
                Ok(DeviceSnapshot {
                    id: DeviceId::new("local/cpu")?,
                    kind: DeviceKind::Cpu,
                    capabilities: DeviceCapabilities {
                        class: DeviceClass::Cpu,
                        memory_kind: DeviceMemoryKind::Unified,
                        compatible_apis: vec![DeviceApi::Cpu],
                    },
                    total_memory_bytes: None,
                    available_memory_bytes: None,
                    used_memory_bytes: None,
                    utilization_percent: None,
                    healthy: true,
                })
            })
            .transpose()?
            .into_iter()
            .collect();
        Ok(ResourceSnapshot {
            logical_cpus,
            devices,
            thermal_pressure: ThermalPressure::Unknown,
            status: ResourceProbeStatus::Unsupported,
            ..ResourceSnapshot::default()
        })
    }
}
