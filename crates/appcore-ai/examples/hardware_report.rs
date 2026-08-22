// =============================================================================
//        #######
//     ###       ###     F: hardware_report.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/22 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/22 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use appcore_ai::{
    AiContributionPolicy, AiResourceMode, ResourceGovernor, ResourceGovernorConfig,
    SystemHardwareProbe,
};
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let interval = Duration::from_millis(250);
    let probe = SystemHardwareProbe::with_sampling_interval(interval)?;
    let _ = probe.refresh()?;
    std::thread::sleep(interval);
    let snapshot = probe.refresh()?;

    println!("AppCore AI hardware report");
    println!(
        "compiled: lightweight=true candle={} openai_compatible={} swarm={} nvidia_probe={}",
        cfg!(feature = "backend-candle"),
        cfg!(feature = "backend-openai-compatible"),
        cfg!(feature = "swarm"),
        cfg!(feature = "accelerator-nvidia")
    );
    println!("status: {:?}", snapshot.status);
    println!(
        "cpu: logical={:?} physical={:?} host_load={:?}% process_load={:?}%",
        snapshot.logical_cpus,
        snapshot.physical_cpus,
        snapshot.cpu_load_percent,
        snapshot.process_cpu_percent
    );
    println!(
        "memory: total={} available={} used={} pressure={:?}%",
        bytes(snapshot.total_memory_bytes),
        bytes(snapshot.available_memory_bytes),
        bytes(snapshot.used_memory_bytes),
        snapshot.memory_pressure_percent
    );
    println!("thermal: {:?}", snapshot.thermal_pressure);
    for device in &snapshot.devices {
        println!(
            "device: {} kind={:?} class={:?} memory={:?} total={} available={} used={} load={:?}% healthy={} apis={:?}",
            device.id,
            device.kind,
            device.capabilities.class,
            device.capabilities.memory_kind,
            bytes(device.total_memory_bytes),
            bytes(device.available_memory_bytes),
            bytes(device.used_memory_bytes),
            device.utilization_percent,
            device.healthy,
            device.capabilities.compatible_apis
        );
    }
    if !snapshot.failures.is_empty() {
        println!("degraded components: {:?}", snapshot.failures);
    }

    let governor = ResourceGovernor::new(
        probe.clone(),
        ResourceGovernorConfig {
            sampling_interval: interval,
            ..ResourceGovernorConfig::default()
        },
        AiContributionPolicy::default(),
    )?;
    for (index, mode) in [
        AiResourceMode::Eco,
        AiResourceMode::Balanced,
        AiResourceMode::Performance,
        AiResourceMode::Unrestricted,
    ]
    .into_iter()
    .enumerate()
    {
        let budget = governor
            .budgets(mode, u64::try_from(index)?)
            .map(|pair| pair.local)?;
        println!(
            "budget {:?}: cpu={}%, gpu={}%, ram={}, vram={}, workers={}, jobs={}, pressure_limited={}",
            mode,
            budget.cpu_percent,
            budget.gpu_percent,
            bytes(budget.memory_bytes),
            bytes(budget.vram_bytes),
            budget.workers,
            budget.concurrent_jobs,
            budget.pressure_limited
        );
    }
    let metrics = probe.metrics();
    println!(
        "sampler: samples={} failures={} cache_hits={} age={:?}",
        metrics.samples, metrics.sample_failures, metrics.cache_hits, metrics.snapshot_age
    );
    Ok(())
}

fn bytes(value: Option<u64>) -> String {
    value.map_or_else(
        || "unknown".to_owned(),
        |bytes| format!("{:.2} GiB", bytes as f64 / 1_073_741_824.0),
    )
}
