// =============================================================================
//        #######
//     ###       ###     F: runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/31 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/31 12:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Measures bounded request allocation, validation, and dispatch ownership.

mod ingress;

use appcore_api::{
    ApiRequest, ApiResponse, ApiRouter, HttpApiConfig, HttpReloadPolicy, QueryEndpoint, QueryName,
    QueryRequest, ReloadableRuntimeHttpHost, RuntimeHttpHost, RuntimeStaticInfo,
};
use std::hint::black_box;
use std::sync::Arc;
use std::time::Instant;

const REQUEST_CASE: &str = "api_request_256b";
const QUERY_VALIDATION_CASE: &str = "query_validation_4mib";
const QUERY_OWNED_CLONE_CASE: &str = "query_owned_clone_4mib";
const QUERY_METADATA_CLONE_CASE: &str = "query_metadata_clone_4mib";
const STATIC_INFO_OWNED_CLONE_CASE: &str = "static_info_owned_clone_1024_peers";
const STATIC_INFO_SHARED_CLONE_CASE: &str = "static_info_shared_clone_1024_peers";
const HTTP_RELOAD_GENERATION_CASE: &str = "http_reload_generation_1024_peers";
const QUERY_NAMES_OWNED_CASE: &str = "query_names_owned_1024";
const QUERY_NAMES_BORROWED_CASE: &str = "query_names_borrowed_1024";
const HTTP_INGRESS_CASE: &str = "http_ingress_16x512kib";
const QUERY_PAYLOAD_BYTES: usize = 4 * 1_024 * 1_024;
const QUERY_JSON_OVERHEAD_BYTES: usize = 11;
const STATIC_INFO_PEERS: usize = 1_024;

const CASES: &[&str] = &[
    REQUEST_CASE,
    QUERY_VALIDATION_CASE,
    QUERY_OWNED_CLONE_CASE,
    QUERY_METADATA_CLONE_CASE,
    STATIC_INFO_OWNED_CLONE_CASE,
    STATIC_INFO_SHARED_CLONE_CASE,
    HTTP_RELOAD_GENERATION_CASE,
    QUERY_NAMES_OWNED_CASE,
    QUERY_NAMES_BORROWED_CASE,
    HTTP_INGRESS_CASE,
];

fn main() -> Result<(), String> {
    memory_checkpoint("idle");
    let selected = std::env::var("APPCORE_BENCH_CASE").ok();
    if selected
        .as_deref()
        .is_none_or(|value| value == REQUEST_CASE)
    {
        measure(REQUEST_CASE, 100_000, || {
            black_box(appcore_api::ApiRequest {
                method: appcore_api::ApiMethod::Query,
                path: "/v1/query/bench.status".to_owned(),
                payload: vec![0x5a; 256],
            });
        });
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == QUERY_VALIDATION_CASE)
    {
        benchmark_query_validation();
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == QUERY_OWNED_CLONE_CASE)
    {
        benchmark_query_owned_clone();
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == QUERY_METADATA_CLONE_CASE)
    {
        benchmark_query_metadata_clone();
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == STATIC_INFO_OWNED_CLONE_CASE)
    {
        benchmark_static_info_owned_clone();
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == STATIC_INFO_SHARED_CLONE_CASE)
    {
        benchmark_static_info_shared_clone();
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == HTTP_RELOAD_GENERATION_CASE)
    {
        benchmark_http_reload_generation()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == QUERY_NAMES_OWNED_CASE)
    {
        benchmark_query_names_owned()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == QUERY_NAMES_BORROWED_CASE)
    {
        benchmark_query_names_borrowed()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == HTTP_INGRESS_CASE)
    {
        ingress::benchmark(HTTP_INGRESS_CASE)?;
    }
    if let Some(value) = selected.as_deref() {
        if !CASES.contains(&value) {
            return Err(format!("unknown appcore-api benchmark case: {value}"));
        }
    }
    memory_checkpoint("retained");
    Ok(())
}

fn benchmark_query_validation() {
    let request = query_fixture();
    let exact_bytes = QUERY_PAYLOAD_BYTES + QUERY_JSON_OVERHEAD_BYTES;
    assert!(request.validate(exact_bytes).is_ok());

    measure(QUERY_VALIDATION_CASE, 10, || {
        let _ = black_box(request.validate(exact_bytes));
    });
}

fn benchmark_query_owned_clone() {
    let request = query_fixture();
    measure(QUERY_OWNED_CLONE_CASE, 10, || {
        black_box(request.clone());
    });
}

fn benchmark_query_metadata_clone() {
    let request = query_fixture();
    measure(QUERY_METADATA_CLONE_CASE, 100_000, || {
        black_box((request.query_id.clone(), request.query_name.clone()));
    });
}

fn query_fixture() -> QueryRequest {
    QueryRequest {
        query_name: "runtime.benchmark".to_string(),
        query_id: "query-benchmark".to_string(),
        payload: serde_json::json!({ "data": "x".repeat(QUERY_PAYLOAD_BYTES) }),
    }
}

fn benchmark_static_info_owned_clone() {
    let static_info = static_info_fixture();
    measure(STATIC_INFO_OWNED_CLONE_CASE, 1_000, || {
        black_box(static_info.clone());
    });
}

fn benchmark_static_info_shared_clone() {
    let static_info = Arc::new(static_info_fixture());
    measure(STATIC_INFO_SHARED_CLONE_CASE, 100_000, || {
        black_box(Arc::clone(&static_info));
    });
}

fn static_info_fixture() -> RuntimeStaticInfo {
    RuntimeStaticInfo {
        app_id: "benchmark-app".to_string(),
        node_id: "benchmark-node".to_string(),
        tenant_id: "benchmark-tenant".to_string(),
        cluster_id: "benchmark-cluster".to_string(),
        core_id: "benchmark-core".to_string(),
        operation_mode: "read_write".to_string(),
        storage_status: "Online".to_string(),
        security_ok: true,
        api_enabled: true,
        sync_enabled: true,
        sync_role: "leader".to_string(),
        sync_log_len: 1_024,
        sync_log_path: Some("/var/lib/appcore/sync.log".to_string()),
        sync_checkpoint_path: Some("/var/lib/appcore/sync.checkpoint".to_string()),
        sync_peers: endpoint_fixture("peer"),
        sync_dns_enabled: true,
        sync_dns_seeds: endpoint_fixture("seed"),
        sync_dns_default_port: 39_201,
        idempotency_ttl_ms: 86_400_000,
        idempotency_path: Some("/var/lib/appcore/idempotency.log".to_string()),
    }
}

fn benchmark_http_reload_generation() -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    let host = ReloadableRuntimeHttpHost::new(
        1,
        RuntimeHttpHost::new(http_config(), static_info_fixture()),
    )
    .map_err(|error| error.to_string())?;
    let policy = HttpReloadPolicy::new(
        std::time::Duration::from_secs(1),
        std::time::Duration::from_secs(1),
    )
    .map_err(|error| error.to_string())?;
    let mut generation = 1_u64;
    measure_fallible(HTTP_RELOAD_GENERATION_CASE, 64, || {
        generation = generation
            .checked_add(1)
            .ok_or_else(|| "HTTP benchmark generation overflow".to_string())?;
        let candidate = host
            .prepare(
                generation,
                RuntimeHttpHost::new(http_config(), static_info_fixture()),
            )
            .map_err(|error| error.to_string())?;
        runtime
            .block_on(host.reload(candidate, policy))
            .map_err(|error| error.to_string())
    })?;
    let generations = host.generation_snapshot();
    if generations.retained_generations != 1 || generations.retiring.is_some() {
        return Err("HTTP benchmark retained a drained routing generation".to_string());
    }
    println!(
        "appcore-api::{HTTP_RELOAD_GENERATION_CASE} retained_generations={} max_retained_generations={} active_inflight={} retiring_inflight=0",
        generations.retained_generations,
        generations.max_retained_generations,
        generations.active.inflight,
    );
    Ok(())
}

fn http_config() -> HttpApiConfig {
    HttpApiConfig {
        host: "127.0.0.1".to_string(),
        port: 39_011,
        enabled: true,
        max_payload_bytes: 65_536,
    }
}

fn benchmark_query_names_owned() -> Result<(), String> {
    let router = query_router_fixture()?;
    measure(QUERY_NAMES_OWNED_CASE, 1_000, || {
        black_box(router.query_names());
    });
    Ok(())
}

fn benchmark_query_names_borrowed() -> Result<(), String> {
    let router = query_router_fixture()?;
    measure(QUERY_NAMES_BORROWED_CASE, 10_000, || {
        black_box(
            router
                .query_names_iter()
                .min_by(|left, right| left.as_str().cmp(right.as_str())),
        );
    });
    Ok(())
}

fn query_router_fixture() -> Result<ApiRouter, String> {
    let mut router = ApiRouter::new();
    for index in 0..STATIC_INFO_PEERS {
        router
            .register_query(BenchmarkQuery {
                name: QueryName::new(format!("benchmark.query.{index:04}"))
                    .map_err(|error| format!("{error:?}"))?,
            })
            .map_err(|error| format!("{error:?}"))?;
    }
    router.freeze_queries();
    Ok(router)
}

struct BenchmarkQuery {
    name: QueryName,
}

impl QueryEndpoint for BenchmarkQuery {
    fn query_name(&self) -> &QueryName {
        &self.name
    }

    fn handle_query(&self, _request: ApiRequest) -> appcore_core::RuntimeResult<ApiResponse> {
        Ok(ApiResponse {
            status_code: 200,
            payload: Vec::new(),
        })
    }
}

fn endpoint_fixture(prefix: &str) -> Vec<String> {
    (0..STATIC_INFO_PEERS)
        .map(|index| format!("{prefix}-{index:04}.runtime.internal"))
        .collect()
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
    let started = benchmark_started();
    for _ in 0..iterations {
        operation();
    }
    let total_ns = started.elapsed().as_nanos();
    println!(
        "appcore-api::{case} iterations={iterations} total_ns={total_ns} ns_per_iter={:.2}",
        total_ns as f64 / iterations as f64
    );
}

fn measure_fallible(
    case: &str,
    fallback: u64,
    mut operation: impl FnMut() -> Result<(), String>,
) -> Result<(), String> {
    let iterations = iterations(fallback);
    let started = benchmark_started();
    for _ in 0..iterations {
        operation()?;
    }
    let total_ns = started.elapsed().as_nanos();
    println!(
        "appcore-api::{case} iterations={iterations} total_ns={total_ns} ns_per_iter={:.2}",
        total_ns as f64 / iterations as f64
    );
    Ok(())
}

fn benchmark_started() -> Instant {
    memory_checkpoint("workload");
    Instant::now()
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
