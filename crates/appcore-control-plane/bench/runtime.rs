// =============================================================================
//        #######
//     ###       ###     F: runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/31 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/31 12:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Measures endpoint validation and bounded file-state backup.

use appcore_control_plane::{
    ControlPlaneError, ControlPlaneHttpConfig, ControlPlaneProvider, ControlPlaneResult,
    CoreRegistration, FileControlPlane, HttpControlPlaneClient, HttpControlPlaneRequest,
    HttpControlPlaneResponse, HttpTransport, InMemoryControlPlane, RetryPolicy,
};
use appcore_core::{
    AppFamily, AppId, ClusterId, CoreId, CoreIdentity, CoreKind, DistributedCoreManifest,
    InstanceId, NodeId, ProtocolVersion, RuntimeContractVersion, RuntimeIdentity,
    RuntimeOperationalMode, SyncGroup, TenantId,
};
use std::collections::BTreeMap;
use std::future::Future;
use std::hint::black_box;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

const ENDPOINT_CASE: &str = "secure_endpoint_validation";
const FILE_BACKUP_CASE: &str = "file_backup_4mib";
const MEMORY_REGISTRATION_CASE: &str = "memory_registration_pressure_32mib";
const HTTP_RETRY_CASE: &str = "http_retry_request_4mib";

#[derive(Clone, Copy)]
struct TimeoutTransport;

impl HttpTransport for TimeoutTransport {
    fn send_json(
        &self,
        _base_url: &str,
        request: HttpControlPlaneRequest,
    ) -> ControlPlaneResult<HttpControlPlaneResponse> {
        black_box(request.body.len());
        Err(ControlPlaneError::Timeout)
    }

    fn send_json_shared_traced_cancellable(
        &self,
        _base_url: &str,
        request: &appcore_control_plane::SharedHttpControlPlaneRequest,
        _trace: Option<&appcore_core::TraceContext>,
        _cancellation: &appcore_control_plane::CancellationToken,
    ) -> ControlPlaneResult<HttpControlPlaneResponse> {
        black_box(request.body().len());
        Err(ControlPlaneError::Timeout)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    memory_checkpoint("idle", true);
    let selected = std::env::var("APPCORE_BENCH_CASE").ok();
    if selected
        .as_deref()
        .is_none_or(|value| value == ENDPOINT_CASE)
    {
        benchmark_endpoint()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == FILE_BACKUP_CASE)
    {
        benchmark_file_backup()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == MEMORY_REGISTRATION_CASE)
    {
        benchmark_memory_registration_pressure()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == HTTP_RETRY_CASE)
    {
        benchmark_http_retry()?;
    }
    if let Some(value) = selected.as_deref() {
        if value != ENDPOINT_CASE
            && value != FILE_BACKUP_CASE
            && value != MEMORY_REGISTRATION_CASE
            && value != HTTP_RETRY_CASE
        {
            return Err(format!("unknown appcore-control-plane benchmark case: {value}").into());
        }
    }
    memory_checkpoint("retained", true);
    Ok(())
}

fn benchmark_http_retry() -> Result<(), Box<dyn std::error::Error>> {
    let client = HttpControlPlaneClient::new(
        ControlPlaneHttpConfig {
            base_url: "https://control.invalid".to_string(),
            timeout_ms: 1,
            retry_policy: RetryPolicy {
                max_attempts: 3,
                initial_backoff_ms: 0,
                max_backoff_ms: 0,
            },
        },
        TimeoutTransport,
    );
    let payload = "x".repeat(4 * 1024 * 1024);
    measure(HTTP_RETRY_CASE, 1, || {
        let mut manifest = manifest();
        manifest
            .metadata
            .insert("benchmark-payload".to_string(), payload.clone());
        let result = block_on(client.register(CoreRegistration {
            manifest,
            registered_at_ms: 0,
            operation_mode: RuntimeOperationalMode::ReadWrite,
        }));
        if result != Err(ControlPlaneError::Timeout) {
            return Err("retry benchmark did not exhaust the configured attempts".into());
        }
        Ok(())
    })
}

fn benchmark_endpoint() -> Result<(), Box<dyn std::error::Error>> {
    measure(ENDPOINT_CASE, 100_000, || {
        black_box(
            appcore_control_plane::require_secure_remote_endpoint(
                "https://control.example.test/v1",
            )
            .map_err(runtime_error)?,
        );
        Ok(())
    })
}

fn benchmark_file_backup() -> Result<(), Box<dyn std::error::Error>> {
    let root = benchmark_root();
    if root.exists() {
        std::fs::remove_dir_all(&root)?;
    }
    let control = FileControlPlane::open(&root, 60_000).map_err(runtime_error)?;
    let mut manifest = manifest();
    manifest
        .metadata
        .insert("benchmark-payload".to_string(), "x".repeat(4 * 1024 * 1024));
    block_on(control.register(CoreRegistration {
        manifest,
        registered_at_ms: 0,
        operation_mode: RuntimeOperationalMode::ReadWrite,
    }))
    .map_err(runtime_error)?;
    let backup = root.join("benchmark.backup");
    let result = measure(FILE_BACKUP_CASE, 10, || {
        if backup.exists() {
            std::fs::remove_file(&backup)?;
        }
        control.backup_to(&backup).map_err(runtime_error)?;
        black_box(std::fs::metadata(&backup)?.len());
        Ok(())
    });
    std::fs::remove_dir_all(root)?;
    result
}

fn benchmark_memory_registration_pressure() -> Result<(), Box<dyn std::error::Error>> {
    let control = InMemoryControlPlane::default();
    measure(MEMORY_REGISTRATION_CASE, 1, || {
        let mut accepted = 0_u64;
        for index in 0..32 {
            let mut manifest = manifest();
            manifest.identity.instance_id = InstanceId::new(format!("benchmark-instance-{index}"))
                .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
            manifest
                .metadata
                .insert("benchmark-payload".to_string(), "x".repeat(1024 * 1024));
            if block_on(control.register(CoreRegistration {
                manifest,
                registered_at_ms: index,
                operation_mode: RuntimeOperationalMode::ReadWrite,
            }))
            .is_ok()
            {
                accepted = accepted.saturating_add(1);
            }
        }
        black_box(accepted);
        Ok(())
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
        "appcore-control-plane::{case_name} iterations={iterations} total_ns={total_ns} ns_per_iter={:.2}",
        total_ns as f64 / iterations as f64
    );
    Ok(())
}

fn manifest() -> DistributedCoreManifest {
    let core_id = CoreId::new("benchmark-core").unwrap();
    DistributedCoreManifest {
        identity: CoreIdentity {
            tenant_id: TenantId::new("benchmark-tenant").unwrap(),
            cluster_id: ClusterId::new("benchmark-cluster").unwrap(),
            core_id: core_id.clone(),
            instance_id: InstanceId::new("benchmark-instance").unwrap(),
            kind: CoreKind::operational(),
            protocol_version: ProtocolVersion::new(1),
            runtime: RuntimeIdentity {
                app_id: AppId::new("benchmark-app").unwrap(),
                app_family: AppFamily::new("benchmark-family").unwrap(),
                sync_group: SyncGroup::new("benchmark-cluster").unwrap(),
                runtime_contract: RuntimeContractVersion::new(1),
                node_id: NodeId::new(core_id.as_str()).unwrap(),
            },
        },
        app_name: "Benchmark".to_string(),
        app_version: "1.0.0".to_string(),
        runtime_min_version: "1.0.0".to_string(),
        runtime_max_version: None,
        capabilities: Vec::new(),
        endpoints: Vec::new(),
        metadata: BTreeMap::new(),
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    let waker = std::task::Waker::noop();
    let mut context = std::task::Context::from_waker(waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            std::task::Poll::Ready(output) => return output,
            std::task::Poll::Pending => std::thread::yield_now(),
        }
    }
}

fn benchmark_root() -> PathBuf {
    std::env::temp_dir().join(format!(
        "appcore-control-plane-benchmark-{}",
        std::process::id()
    ))
}

fn runtime_error(error: appcore_control_plane::ControlPlaneError) -> Box<dyn std::error::Error> {
    Box::new(std::io::Error::other(format!("{error:?}")))
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
