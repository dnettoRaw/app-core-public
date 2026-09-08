// =============================================================================
//        #######
//     ###       ###     F: runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/31 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/31 12:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Measures deterministic Gateway hashing, selection, and bounded registry scans.

use appcore_contracts::InstallationId;
use appcore_gateway::{
    CapabilityRegistry, CapabilityResolver, GatewayConfig, GatewayState, TenantState,
    WorkerConnection, WorkerConnectionKey, WorkerSelectionInput, WorkerSelectionPolicy,
};
use appcore_security::HashTokenProvider;
use appcore_types::{CapabilityName, ClusterId, CoreId, InstanceId, TenantId};
use std::collections::{HashMap, HashSet};
use std::hint::black_box;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

const CONNECTION_HASH_CASE: &str = "client_connection_hash";
const WORKER_CONNECTION_HASH_CASE: &str = "worker_connection_hash_64_capabilities";
const CONNECTION_COUNT_CASE: &str = "connection_count_1024_tenants";
const OWNED_CAPABILITIES_CASE: &str = "capability_registry_owned_1024_workers";
const SHARED_CAPABILITIES_CASE: &str = "capability_registry_shared_1024_workers";
const WORKER_SELECTION_CASE: &str = "worker_selection_round_robin_1024";
const BENCHMARK_WORKERS: usize = 1_024;
const BENCHMARK_CAPABILITIES: usize = 64;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    memory_checkpoint("idle");
    let selected = std::env::var("APPCORE_BENCH_CASE").ok();
    if selected
        .as_deref()
        .is_none_or(|value| value == CONNECTION_HASH_CASE)
    {
        benchmark_connection_hash();
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == WORKER_CONNECTION_HASH_CASE)
    {
        benchmark_worker_connection_hash()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == CONNECTION_COUNT_CASE)
    {
        benchmark_connection_count()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == OWNED_CAPABILITIES_CASE)
    {
        benchmark_owned_capability_registry()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == SHARED_CAPABILITIES_CASE)
    {
        benchmark_shared_capability_registry()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == WORKER_SELECTION_CASE)
    {
        benchmark_worker_selection()?;
    }
    if let Some(value) = selected.as_deref() {
        if ![
            CONNECTION_HASH_CASE,
            WORKER_CONNECTION_HASH_CASE,
            CONNECTION_COUNT_CASE,
            OWNED_CAPABILITIES_CASE,
            SHARED_CAPABILITIES_CASE,
            WORKER_SELECTION_CASE,
        ]
        .contains(&value)
        {
            return Err(format!("unknown appcore-gateway benchmark case: {value}").into());
        }
    }
    memory_checkpoint("retained");
    Ok(())
}

fn benchmark_connection_hash() {
    let tenant = TenantId::new("tenant-bench").expect("benchmark tenant ID must be valid");
    let cluster = ClusterId::new("cluster-bench").expect("benchmark cluster ID must be valid");
    let device = InstanceId::new("device-bench").expect("benchmark device ID must be valid");
    measure(CONNECTION_HASH_CASE, 50_000, || {
        black_box(appcore_gateway::client_connection_hash(
            &tenant, &cluster, &device,
        ));
    });
}

fn benchmark_worker_connection_hash() -> Result<(), String> {
    let tenant = TenantId::new("tenant-worker-hash").map_err(debug_error)?;
    let cluster = ClusterId::new("cluster-worker-hash").map_err(debug_error)?;
    let installation = InstallationId::new("installation-worker-hash").map_err(debug_error)?;
    let core = CoreId::new("core-worker-hash").map_err(debug_error)?;
    let capabilities = (0..BENCHMARK_CAPABILITIES)
        .map(|index| {
            let prefix = format!("runtime.benchmark.{index:02}.");
            CapabilityName::new(format!(
                "{prefix}{}",
                "x".repeat(128_usize.saturating_sub(prefix.len()))
            ))
            .map_err(debug_error)
        })
        .collect::<Result<Vec<_>, _>>()?;
    measure(WORKER_CONNECTION_HASH_CASE, 10_000, || {
        black_box(appcore_gateway::worker_connection_hash(
            &tenant,
            &cluster,
            &installation,
            &core,
            &capabilities,
        ));
    });
    Ok(())
}

fn benchmark_connection_count() -> Result<(), Box<dyn std::error::Error>> {
    let state = gateway_fixture()?;
    measure(CONNECTION_COUNT_CASE, 1_000, || {
        black_box(state.connection_count());
    });
    Ok(())
}

fn benchmark_owned_capability_registry() -> Result<(), Box<dyn std::error::Error>> {
    let (registry, capability) = owned_capability_fixture()?;
    measure(OWNED_CAPABILITIES_CASE, 10_000, || {
        black_box(resolve_owned(&registry, &capability));
    });
    Ok(())
}

fn benchmark_shared_capability_registry() -> Result<(), Box<dyn std::error::Error>> {
    let (registry, capability) = shared_capability_fixture()?;
    let stats = registry.stats();
    assert_eq!(stats.unique_capabilities, BENCHMARK_CAPABILITIES);
    assert_eq!(stats.workers, BENCHMARK_WORKERS);
    assert_eq!(
        stats.advertisements,
        BENCHMARK_WORKERS * BENCHMARK_CAPABILITIES
    );
    measure(SHARED_CAPABILITIES_CASE, 10_000, || {
        black_box(resolve_shared(&registry, &capability));
    });
    Ok(())
}

fn benchmark_worker_selection() -> Result<(), Box<dyn std::error::Error>> {
    let tenant_id = TenantId::new("tenant-selection-benchmark")
        .map_err(|error| format!("benchmark tenant failed: {error:?}"))?;
    let capability = CapabilityName::new("runtime.benchmark.selection")
        .map_err(|error| format!("benchmark capability failed: {error:?}"))?;
    let mut tenant = TenantState::new(tenant_id.clone());
    let mut receivers = Vec::with_capacity(BENCHMARK_WORKERS);
    for index in 0..BENCHMARK_WORKERS {
        let (sender, receiver) = mpsc::channel(128);
        tenant.add_worker(
            WorkerConnection::new(
                WorkerConnectionKey {
                    tenant_id: tenant_id.clone(),
                    installation_id: InstallationId::new(format!("installation-{index:04}"))?,
                    core_id: CoreId::new(format!("core-{index:04}"))
                        .map_err(|error| format!("benchmark core failed: {error:?}"))?,
                },
                sender,
                1_000_000,
            ),
            vec![capability.clone()],
        )?;
        receivers.push(receiver);
    }
    let resolver = CapabilityResolver::with_policy(WorkerSelectionPolicy::RoundRobin);
    let input = WorkerSelectionInput::new(1_000_000, Duration::from_secs(10));
    measure(WORKER_SELECTION_CASE, 1_024, || {
        black_box(
            resolver
                .select(&capability, &tenant, input)
                .expect("benchmark worker selection must succeed"),
        );
    });
    black_box(receivers);
    Ok(())
}

#[inline(never)]
fn resolve_owned(registry: &OwnedCapabilityRegistry, capability: &CapabilityName) -> Option<usize> {
    registry.resolve_count(capability)
}

#[inline(never)]
fn resolve_shared(registry: &CapabilityRegistry, capability: &CapabilityName) -> Option<usize> {
    registry.resolve(capability).map(HashSet::len)
}

fn owned_capability_fixture() -> Result<(OwnedCapabilityRegistry, CapabilityName), String> {
    let capabilities = benchmark_capabilities()?;
    let mut registry = OwnedCapabilityRegistry::default();
    for index in 0..BENCHMARK_WORKERS {
        registry.register(benchmark_worker(index)?, capabilities.clone());
    }
    Ok((registry, capabilities[0].clone()))
}

fn shared_capability_fixture() -> Result<(CapabilityRegistry, CapabilityName), String> {
    let capabilities = benchmark_capabilities()?;
    let mut registry = CapabilityRegistry::new();
    for index in 0..BENCHMARK_WORKERS {
        registry.register(benchmark_worker(index)?, capabilities.clone());
    }
    Ok((registry, capabilities[0].clone()))
}

fn benchmark_capabilities() -> Result<Vec<CapabilityName>, String> {
    (0..BENCHMARK_CAPABILITIES)
        .map(|index| {
            CapabilityName::new(format!("runtime.benchmark.capability-{index:02}"))
                .map_err(|error| format!("benchmark capability failed: {error:?}"))
        })
        .collect()
}

fn benchmark_worker(index: usize) -> Result<WorkerConnectionKey, String> {
    Ok(WorkerConnectionKey {
        tenant_id: TenantId::new("tenant-capability-benchmark")
            .map_err(|error| format!("benchmark tenant failed: {error:?}"))?,
        installation_id: InstallationId::new(format!("installation-{index:04}"))
            .map_err(|error| format!("benchmark installation failed: {error:?}"))?,
        core_id: CoreId::new(format!("core-{index:04}"))
            .map_err(|error| format!("benchmark core failed: {error:?}"))?,
    })
}

fn debug_error(error: impl std::fmt::Debug) -> String {
    format!("benchmark fixture failed: {error:?}")
}

#[derive(Default)]
struct OwnedCapabilityRegistry {
    capability_to_workers: HashMap<CapabilityName, HashSet<WorkerConnectionKey>>,
    worker_to_capabilities: HashMap<WorkerConnectionKey, HashSet<CapabilityName>>,
}

impl OwnedCapabilityRegistry {
    fn register(&mut self, worker: WorkerConnectionKey, capabilities: Vec<CapabilityName>) {
        let mut worker_capabilities = HashSet::new();
        for capability in capabilities {
            self.capability_to_workers
                .entry(capability.clone())
                .or_default()
                .insert(worker.clone());
            worker_capabilities.insert(capability);
        }
        self.worker_to_capabilities
            .insert(worker, worker_capabilities);
    }

    fn resolve_count(&self, capability: &CapabilityName) -> Option<usize> {
        self.capability_to_workers.get(capability).map(HashSet::len)
    }
}

fn gateway_fixture() -> Result<GatewayState, String> {
    let config = GatewayConfig::new(([127, 0, 0, 1], 8080).into(), "gateway.benchmark.local");
    let provider = HashTokenProvider::from_secret(vec![7; 32])
        .map_err(|error| format!("benchmark token provider failed: {error:?}"))?;
    let state = GatewayState::new(config, provider)
        .map_err(|error| format!("benchmark Gateway state failed: {error}"))?;
    for index in 0..1_024 {
        let tenant = TenantId::new(format!("tenant-benchmark-{index:04}"))
            .map_err(|error| format!("benchmark tenant ID failed: {error:?}"))?;
        state
            .tenant_partition_or_insert(&tenant)
            .map_err(|error| format!("benchmark tenant insertion failed: {error}"))?;
    }
    assert_eq!(state.tenant_count(), 1_024);
    Ok(state)
}

fn iterations(fallback: u64) -> u64 {
    std::env::var("APPCORE_BENCH_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}

fn measure(case: &str, fallback: u64, mut operation: impl FnMut()) {
    let iterations = iterations(fallback);
    memory_checkpoint("workload");
    let started = Instant::now();
    for _ in 0..iterations {
        operation();
    }
    let total_ns = started.elapsed().as_nanos();
    println!(
        "appcore-gateway::{case} iterations={iterations} total_ns={total_ns} ns_per_iter={:.2}",
        total_ns as f64 / iterations as f64
    );
}
fn memory_checkpoint(phase: &str) {
    let Some(milliseconds) = checkpoint_milliseconds() else {
        return;
    };
    println!(
        "appcore-bench-memory phase={phase} pid={}",
        std::process::id()
    );
    let _ = std::io::Write::flush(&mut std::io::stdout());
    std::thread::sleep(std::time::Duration::from_millis(milliseconds));
}
fn checkpoint_milliseconds() -> Option<u64> {
    std::env::var("APPCORE_BENCH_MEMORY_CHECKPOINT_MS")
        .ok()?
        .parse()
        .ok()
        .filter(|value| (1..=1_000).contains(value))
}
