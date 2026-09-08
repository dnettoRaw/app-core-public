// =============================================================================
//        #######
//     ###       ###     F: runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/31 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/31 12:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Measures deterministic placement across bounded Core candidates.

use appcore_contracts::{
    CapabilityId, CoreId, CoreProfile, CoreRole, LeadershipMode, LeadershipRequirement,
    ResourceProfile, RuntimeHealthStatus, RuntimeMode, RuntimeOperationalMode, SchedulingProfile,
    ServiceId, WorkloadClass,
};
use appcore_scheduler::{
    FileSchedulerStateProvider, PlacementCandidate, PlacementEngine, PlacementRequest,
    ResourceRequest, SchedulerStateProvider, SCHEDULER_STATE_FORMAT_V1,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::hint::black_box;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

const PLACEMENT_CASE: &str = "placement_4_candidates";
const STATE_CASE: &str = "state_snapshot_read_1024";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    memory_checkpoint("idle", true);
    let selected = std::env::var("APPCORE_BENCH_CASE").ok();
    if selected
        .as_deref()
        .is_none_or(|value| value == PLACEMENT_CASE)
    {
        benchmark_placement()?;
    }
    if selected.as_deref().is_none_or(|value| value == STATE_CASE) {
        benchmark_state_file()?;
    }
    if let Some(value) = selected.as_deref() {
        if value != PLACEMENT_CASE && value != STATE_CASE {
            return Err(format!("unknown appcore-scheduler benchmark case: {value}").into());
        }
    }
    memory_checkpoint("retained", true);
    Ok(())
}

fn benchmark_placement() -> Result<(), Box<dyn std::error::Error>> {
    let request = request()?;
    let candidates = [
        candidate("core-a", 10, 7)?,
        candidate("core-b", 20, 1)?,
        candidate("core-c", 15, 3)?,
        candidate("core-d", 8, 0)?,
    ];
    measure(PLACEMENT_CASE, 20_000, || {
        black_box(PlacementEngine.select(black_box(&request), black_box(&candidates)));
        Ok(())
    })
}

fn benchmark_state_file() -> Result<(), Box<dyn std::error::Error>> {
    let root = benchmark_root();
    if root.exists() {
        std::fs::remove_dir_all(&root)?;
    }
    std::fs::create_dir_all(&root)?;
    let path = root.join("scheduler.json");
    write_state_fixture(&path)?;
    let provider = FileSchedulerStateProvider::new(&path)?;
    let result = measure(STATE_CASE, 100, || {
        black_box(provider.stats()?);
        Ok(())
    });
    std::fs::remove_dir_all(root)?;
    result
}

fn request() -> Result<PlacementRequest, Box<dyn std::error::Error>> {
    Ok(PlacementRequest {
        capability: CapabilityId::new("document.extract")?,
        service_id: ServiceId::new("document.extract")?,
        runtime_mode: RuntimeMode::Cluster,
        requires_write: true,
        requires_leader: true,
        workload: WorkloadClass::Compute,
        affinity: BTreeSet::from(["region.local".to_string()]),
        resources: ResourceRequest {
            cpu_cores: Some(4),
            memory_bytes: Some(8_000),
            gpu_count: 1,
        },
    })
}

fn candidate(
    core: &str,
    weight: u16,
    load: u32,
) -> Result<PlacementCandidate, Box<dyn std::error::Error>> {
    let service = ServiceId::new("document.extract")?;
    let scheduling = SchedulingProfile::new(weight, 1, 8, WorkloadClass::Compute)?
        .with_affinity("region.local")?;
    let profile = CoreProfile::new(
        CoreRole::Compute,
        service.clone(),
        [CapabilityId::new("document.extract")?],
        LeadershipRequirement::new(service.clone(), LeadershipMode::Required, 30_000)?,
        ResourceProfile::new(Some(8), Some(16_000), 1),
        scheduling,
    )?;
    Ok(PlacementCandidate {
        core_id: CoreId::new(core)?,
        runtime_mode: RuntimeMode::Cluster,
        operational_mode: RuntimeOperationalMode::ReadWrite,
        health: RuntimeHealthStatus::Healthy,
        current_load: load,
        leader_services: BTreeSet::from([service]),
        profile,
    })
}

fn iterations(fallback: u64) -> u64 {
    std::env::var("APPCORE_BENCH_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}

fn measure(
    case_name: &str,
    fallback_iterations: u64,
    mut operation: impl FnMut() -> Result<(), Box<dyn std::error::Error>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let iterations = iterations(fallback_iterations);
    memory_checkpoint("workload", true);
    let started = Instant::now();
    for _ in 0..iterations {
        operation()?;
    }
    let total_ns = started.elapsed().as_nanos();
    println!(
        "appcore-scheduler::{case_name} iterations={iterations} total_ns={total_ns} ns_per_iter={:.2}",
        total_ns as f64 / iterations as f64
    );
    Ok(())
}

#[derive(Serialize)]
struct FixtureState<'a> {
    format: &'a str,
    records: &'a [FixtureRecord],
    checksum: String,
}

#[derive(Serialize)]
struct FixtureRecord {
    task_id: String,
    definition_hash: String,
    next_run_ms: u64,
    attempts: u32,
    misfire_policy: &'static str,
    completed: bool,
    last_receipt_epoch: Option<u64>,
    claim: Option<()>,
    fencing_epoch: u64,
}

fn write_state_fixture(path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let records = (0..1_024)
        .map(|index| FixtureRecord {
            task_id: format!("task-{index:04}"),
            definition_hash: "a".repeat(64),
            next_run_ms: index,
            attempts: 0,
            misfire_policy: "fire_once",
            completed: false,
            last_receipt_epoch: None,
            claim: None,
            fencing_epoch: 0,
        })
        .collect::<Vec<_>>();
    let checksum = hex_digest(Sha256::digest(serde_json::to_vec(&records)?).into());
    std::fs::write(
        path,
        serde_json::to_vec(&FixtureState {
            format: SCHEDULER_STATE_FORMAT_V1,
            records: &records,
            checksum,
        })?,
    )?;
    Ok(())
}

fn hex_digest(digest: [u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn benchmark_root() -> PathBuf {
    std::env::temp_dir().join(format!(
        "appcore-scheduler-benchmark-{}",
        std::process::id()
    ))
}

fn memory_checkpoint(phase: &str, settle: bool) {
    let Some(milliseconds) = checkpoint_milliseconds() else {
        return;
    };
    println!(
        "appcore-bench-memory phase={phase} pid={}",
        std::process::id()
    );
    let _ = std::io::stdout().flush();
    if settle {
        std::thread::sleep(std::time::Duration::from_millis(milliseconds));
    }
}
fn checkpoint_milliseconds() -> Option<u64> {
    std::env::var("APPCORE_BENCH_MEMORY_CHECKPOINT_MS")
        .ok()?
        .parse()
        .ok()
        .filter(|value| (1..=1_000).contains(value))
}
