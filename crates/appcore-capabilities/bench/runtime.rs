// =============================================================================
//        #######
//     ###       ###     F: runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/31 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/31 12:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Measures capability contracts and default resolution across many peers.

use appcore_capabilities::{
    CapabilityRegistry, CapabilityRequest, CapabilityResolver, CapabilityResponse,
    CapabilityResult, PeerRpcRemoteCapabilityInvoker, RemoteCapabilityInvoker,
};
use appcore_contracts::ServiceId;
use appcore_core::{
    AppFamily, AppId, CapabilityDescriptor, CapabilityMode, CapabilityName, CapabilityVisibility,
    ClusterId, CoreId, CoreIdentity, CoreKind, InstanceId, NodeId, PeerEndpoint, ProtocolVersion,
    RuntimeContractVersion, RuntimeIdentity, SyncGroup, TenantId,
};
use appcore_distributed_contracts::{
    PeerRecord, PeerRpcCallKind, PeerRpcClientExecutor, PeerRpcError, PeerRpcOutboundRequest,
    PeerRpcResponse,
};
use std::collections::BTreeMap;
use std::hint::black_box;
use std::time::Instant;

const REQUIREMENTS_CASE: &str = "read_only_requirements";
const RESOLVER_CASE: &str = "default_resolver_1024_peers";
const REMOTE_BORROWED_CASE: &str = "remote_invocation_borrowed_4mib";
const REMOTE_OWNED_CASE: &str = "remote_invocation_owned_4mib";
const PROVIDER_OWNED_CASE: &str = "remote_provider_owned_1024_capabilities";
const PROVIDER_BORROWED_CASE: &str = "remote_provider_borrowed_1024_capabilities";
const REMOTE_PAYLOAD_BYTES: usize = 4 * 1_024 * 1_024;

struct EchoPeerRpcClient;

struct EmptyRemoteInvoker;

impl PeerRpcClientExecutor for EchoPeerRpcClient {
    fn call_peer(
        &self,
        _endpoint_url: &str,
        _kind: PeerRpcCallKind,
        request: PeerRpcOutboundRequest,
    ) -> Result<PeerRpcResponse, PeerRpcError> {
        Ok(PeerRpcResponse::ok(request.request_id, request.payload))
    }
}

impl RemoteCapabilityInvoker for EmptyRemoteInvoker {
    fn invoke_remote(
        &self,
        _peer: &PeerRecord,
        _request: &CapabilityRequest,
    ) -> CapabilityResult<CapabilityResponse> {
        Ok(CapabilityResponse::accepted(Vec::new(), None))
    }
}

fn main() -> Result<(), String> {
    memory_checkpoint("idle");
    let selected = std::env::var("APPCORE_BENCH_CASE").ok();
    if selected
        .as_deref()
        .is_none_or(|value| value == REQUIREMENTS_CASE)
    {
        measure(REQUIREMENTS_CASE, 100_000, || {
            black_box(appcore_capabilities::requirements_for_read_only());
        });
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == RESOLVER_CASE)
    {
        benchmark_default_resolver()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == REMOTE_BORROWED_CASE)
    {
        benchmark_remote_invocation(false)?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == REMOTE_OWNED_CASE)
    {
        benchmark_remote_invocation(true)?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == PROVIDER_OWNED_CASE)
    {
        benchmark_remote_provider(false)?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == PROVIDER_BORROWED_CASE)
    {
        benchmark_remote_provider(true)?;
    }
    if let Some(value) = selected.as_deref() {
        if ![
            REQUIREMENTS_CASE,
            RESOLVER_CASE,
            REMOTE_BORROWED_CASE,
            REMOTE_OWNED_CASE,
            PROVIDER_OWNED_CASE,
            PROVIDER_BORROWED_CASE,
        ]
        .contains(&value)
        {
            return Err(format!(
                "unknown appcore-capabilities benchmark case: {value}"
            ));
        }
    }
    memory_checkpoint("retained");
    Ok(())
}

fn benchmark_remote_provider(borrowed: bool) -> Result<(), String> {
    let identity = identity("core-local")?;
    let service_id = ServiceId::new("runtime.resolve").map_err(debug_error)?;
    let request = CapabilityRequest {
        request_id: "request-provider-benchmark".to_string(),
        capability: CapabilityName::new("runtime.resolve").map_err(debug_error)?,
        mode: CapabilityMode::Query,
        payload: Vec::new(),
        idempotency_key: None,
        trace: None,
    };
    let resolver = CapabilityResolver::new(CapabilityRegistry::new())
        .with_peers(vec![large_capability_peer()?]);
    let invoker = EmptyRemoteInvoker;
    let case = if borrowed {
        PROVIDER_BORROWED_CASE
    } else {
        PROVIDER_OWNED_CASE
    };

    measure(case, 100, || {
        if borrowed {
            let _ = black_box(resolver.handle(
                &identity,
                &service_id,
                &request,
                None,
                Some(&invoker),
                0,
            ));
        } else {
            let _ = black_box(resolver.resolve(&identity, &service_id, &request, None, 0));
        }
    });
    Ok(())
}

fn benchmark_remote_invocation(owned: bool) -> Result<(), String> {
    let identity = identity("core-local")?;
    let service_id = ServiceId::new("runtime.resolve").map_err(debug_error)?;
    let capability = CapabilityName::new("runtime.resolve").map_err(debug_error)?;
    let resolver =
        CapabilityResolver::new(CapabilityRegistry::new()).with_peers(vec![peer_fixture(1)?
            .pop()
            .ok_or("peer fixture is empty")?]);
    let invoker = PeerRpcRemoteCapabilityInvoker::new(EchoPeerRpcClient);
    let case = if owned {
        REMOTE_OWNED_CASE
    } else {
        REMOTE_BORROWED_CASE
    };

    measure(case, 16, || {
        let request = CapabilityRequest {
            request_id: "request-benchmark".to_string(),
            capability: capability.clone(),
            mode: CapabilityMode::Query,
            payload: vec![0x5a; REMOTE_PAYLOAD_BYTES],
            idempotency_key: None,
            trace: None,
        };
        if owned {
            let _ = black_box(resolver.handle_owned(
                &identity,
                &service_id,
                request,
                None,
                Some(&invoker),
                0,
            ));
        } else {
            let _ = black_box(resolver.handle(
                &identity,
                &service_id,
                &request,
                None,
                Some(&invoker),
                0,
            ));
        }
    });
    Ok(())
}

fn benchmark_default_resolver() -> Result<(), String> {
    let identity = identity("core-local")?;
    let service_id = ServiceId::new("runtime.resolve").map_err(debug_error)?;
    let request = CapabilityRequest {
        request_id: "request-benchmark".to_string(),
        capability: CapabilityName::new("runtime.resolve").map_err(debug_error)?,
        mode: CapabilityMode::Query,
        payload: Vec::new(),
        idempotency_key: None,
        trace: None,
    };
    let resolver =
        CapabilityResolver::new(CapabilityRegistry::new()).with_peers(peer_fixture(1_024)?);
    resolver
        .resolve(&identity, &service_id, &request, None, 0)
        .map_err(debug_error)?;

    measure(RESOLVER_CASE, 100, || {
        let _ = black_box(resolver.resolve(&identity, &service_id, &request, None, 0));
    });
    Ok(())
}

fn peer_fixture(count: usize) -> Result<Vec<PeerRecord>, String> {
    let capabilities = capability_fixture()?;
    (0..count)
        .map(|index| {
            let mut metadata = BTreeMap::new();
            if index + 1 == count {
                metadata.insert("preferred".to_string(), "true".to_string());
            }
            Ok(PeerRecord {
                identity: identity(&format!("peer-{index:04}"))?,
                endpoints: vec![PeerEndpoint {
                    name: "peer-rpc".to_string(),
                    url: format!("http://127.0.0.1:{index}"),
                    protocol: "appcore-peer-rpc".to_string(),
                    metadata: BTreeMap::new(),
                }],
                capabilities: capabilities.clone(),
                healthy: true,
                last_seen_ms: index as u64,
                metadata,
            })
        })
        .collect()
}

fn capability_fixture() -> Result<Vec<CapabilityDescriptor>, String> {
    let mut capabilities = Vec::with_capacity(9);
    capabilities.push(descriptor("runtime.resolve")?);
    for index in 0..8 {
        capabilities.push(descriptor(&format!("runtime.filler-{index}"))?);
    }
    Ok(capabilities)
}

fn large_capability_peer() -> Result<PeerRecord, String> {
    let mut peer = peer_fixture(1)?.pop().ok_or("peer fixture is empty")?;
    let mut capabilities = Vec::with_capacity(1_024);
    for index in 0..1_023 {
        capabilities.push(descriptor(&format!("runtime.filler-{index:04}"))?);
    }
    capabilities.push(descriptor("runtime.resolve")?);
    peer.capabilities = capabilities;
    Ok(peer)
}

fn descriptor(name: &str) -> Result<CapabilityDescriptor, String> {
    Ok(CapabilityDescriptor::new(
        CapabilityName::new(name).map_err(debug_error)?,
        "1",
        CapabilityMode::Query,
        CapabilityVisibility::Cluster,
    ))
}

fn identity(core_id: &str) -> Result<CoreIdentity, String> {
    Ok(CoreIdentity {
        tenant_id: TenantId::new("tenant-benchmark").map_err(debug_error)?,
        cluster_id: ClusterId::new("cluster-benchmark").map_err(debug_error)?,
        core_id: CoreId::new(core_id).map_err(debug_error)?,
        instance_id: InstanceId::new(format!("{core_id}-instance")).map_err(debug_error)?,
        kind: CoreKind::operational(),
        protocol_version: ProtocolVersion::new(1),
        runtime: RuntimeIdentity {
            app_id: AppId::new("app-benchmark").map_err(debug_error)?,
            app_family: AppFamily::new("family-benchmark").map_err(debug_error)?,
            sync_group: SyncGroup::new("cluster-benchmark").map_err(debug_error)?,
            runtime_contract: RuntimeContractVersion::new(1),
            node_id: NodeId::new(core_id).map_err(debug_error)?,
        },
    })
}

fn debug_error(error: impl std::fmt::Debug) -> String {
    format!("{error:?}")
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
        "appcore-capabilities::{case} iterations={iterations} total_ns={total_ns} ns_per_iter={:.2}",
        total_ns as f64 / iterations as f64
    );
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
