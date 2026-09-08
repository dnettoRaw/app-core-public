// =============================================================================
//        #######
//     ###       ###     F: runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/31 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/03 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Measures bounded Peer RPC codecs, client ownership, replay, and nonce state.

use std::hint::black_box;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

const HASH_CASE: &str = "payload_hash_1k";
const STREAM_SIGNING_CASE: &str = "stream_json_signing_64k";
const NONCE_CASE: &str = "nonce_state_read_65536";
const REPLAY_CASE: &str = "bounded_replay_store_10000";
const V1_HOST_DECODE_CASE: &str = "v1_host_decode_4mib";
const V1_CLIENT_CASE: &str = "v1_client_request_4mib";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    memory_checkpoint("idle", true);
    let selected = std::env::var("APPCORE_BENCH_CASE").ok();
    if selected.as_deref().is_none_or(|value| value == HASH_CASE) {
        benchmark_hash()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == STREAM_SIGNING_CASE)
    {
        benchmark_stream_signing()?;
    }
    if selected.as_deref().is_none_or(|value| value == NONCE_CASE) {
        benchmark_nonce_state()?;
    }
    if selected.as_deref().is_none_or(|value| value == REPLAY_CASE) {
        benchmark_replay_store()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == V1_HOST_DECODE_CASE)
    {
        benchmark_v1_host_decode()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == V1_CLIENT_CASE)
    {
        benchmark_v1_client()?;
    }
    if let Some(value) = selected.as_deref() {
        if value != HASH_CASE
            && value != STREAM_SIGNING_CASE
            && value != NONCE_CASE
            && value != REPLAY_CASE
            && value != V1_HOST_DECODE_CASE
            && value != V1_CLIENT_CASE
        {
            return Err(format!("unknown appcore-peer-rpc benchmark case: {value}").into());
        }
    }
    memory_checkpoint("retained", true);
    Ok(())
}

#[derive(Clone, Copy)]
struct BenchmarkTokenIssuer;

impl appcore_peer_rpc::PeerRpcTokenIssuer for BenchmarkTokenIssuer {
    fn issue_peer_token(
        &self,
        _request_id: &str,
        _request_hash: Option<&str>,
        _now_ms: u64,
        _ttl_ms: u64,
    ) -> Result<String, appcore_peer_rpc::PeerRpcError> {
        Ok("benchmark-token".to_string())
    }
}

#[derive(Clone, Copy)]
struct BenchmarkTransport;

impl appcore_peer_rpc::PeerTransportProvider for BenchmarkTransport {
    fn send(
        &self,
        _base_url: &str,
        request: appcore_peer_rpc::PeerRpcHttpRequest,
    ) -> Result<appcore_peer_rpc::PeerRpcHttpResponse, appcore_peer_rpc::PeerRpcError> {
        black_box(request.body.len());
        Ok(appcore_peer_rpc::PeerRpcHttpResponse {
            status_code: 200,
            body:
                br#"{"ok":true,"request_id":"request-client-benchmark","payload":[],"error":null}"#
                    .to_vec(),
        })
    }
}

fn benchmark_v1_client() -> Result<(), Box<dyn std::error::Error>> {
    use appcore_core::{
        AppFamily, AppId, CapabilityName, CoreIdentity, NodeId, RuntimeContractVersion,
        RuntimeIdentity, SyncGroup,
    };
    use appcore_peer_rpc::{
        PeerRpcCallKind, PeerRpcClient, PeerRpcClientConfig, PeerRpcClientExecutor,
        PeerRpcOutboundRequest,
    };

    let runtime_error = |error| format!("{error:?}");
    let runtime = RuntimeIdentity {
        app_id: AppId::new("app-client-benchmark").map_err(runtime_error)?,
        app_family: AppFamily::new("family-client-benchmark").map_err(runtime_error)?,
        sync_group: SyncGroup::new("group-client-benchmark").map_err(runtime_error)?,
        runtime_contract: RuntimeContractVersion::new(1),
        node_id: NodeId::new("node-client-benchmark").map_err(runtime_error)?,
    };
    let identity = CoreIdentity::from_runtime_defaults(runtime).map_err(runtime_error)?;
    let target = identity.core_id.clone();
    let client = PeerRpcClient::new(
        identity,
        PeerRpcClientConfig::default(),
        BenchmarkTransport,
        BenchmarkTokenIssuer,
    );
    let payload = vec![0x5a; 4 * 1024 * 1024];
    measure(V1_CLIENT_CASE, 1, || {
        let request = PeerRpcOutboundRequest::new(
            "request-client-benchmark",
            target.clone(),
            CapabilityName::new("runtime.benchmark").map_err(runtime_error)?,
            payload.clone(),
            None,
            None,
        );
        black_box(client.call_peer("http://127.0.0.1:8080", PeerRpcCallKind::Query, request)?);
        Ok(())
    })
}

fn benchmark_v1_host_decode() -> Result<(), Box<dyn std::error::Error>> {
    use appcore_core::{CapabilityName, ClusterId, CoreId, TenantId};
    use appcore_peer_rpc::PeerRpcEnvelope;

    let envelope = PeerRpcEnvelope::new(
        "request-benchmark",
        "trace-benchmark",
        CoreId::new("source-benchmark").map_err(|error| format!("{error:?}"))?,
        CoreId::new("target-benchmark").map_err(|error| format!("{error:?}"))?,
        TenantId::new("tenant-benchmark").map_err(|error| format!("{error:?}"))?,
        ClusterId::new("cluster-benchmark").map_err(|error| format!("{error:?}"))?,
        1,
        2,
        "nonce-benchmark",
        CapabilityName::new("runtime.benchmark").map_err(|error| format!("{error:?}"))?,
        vec![0x5a; 4 * 1024 * 1024],
        None,
        None,
    );
    let encoded = serde_json::to_vec(&envelope)?;
    drop(envelope);
    measure(V1_HOST_DECODE_CASE, 3, || {
        let decoded = appcore_peer_rpc::decode_peer_rpc_envelope_json(&encoded, encoded.len())?;
        black_box(decoded);
        Ok(())
    })
}

fn benchmark_stream_signing() -> Result<(), Box<dyn std::error::Error>> {
    use appcore_core::ProtocolVersion;
    use appcore_peer_rpc::v2::{
        PeerRpcChunkEncodingV2, PeerRpcStreamChunkV2, PeerRpcStreamFrameV2,
        PEER_RPC_PROTOCOL_VERSION_V2,
    };

    let payload = vec![0x5a; 64 * 1024];
    let chunk_hash = appcore_peer_rpc::payload_hash(&payload);
    let frame = PeerRpcStreamFrameV2::Chunk(PeerRpcStreamChunkV2 {
        protocol_version: ProtocolVersion::new(PEER_RPC_PROTOCOL_VERSION_V2),
        request_id: "request-signing-benchmark".to_string(),
        stream_id: "stream-signing-benchmark".to_string(),
        sequence: 0,
        encoding: PeerRpcChunkEncodingV2::Identity,
        payload,
        decoded_bytes: 64 * 1024,
        chunk_hash,
    });
    measure(STREAM_SIGNING_CASE, 500, || {
        black_box(appcore_peer_rpc::stream_frame_signing_hash(black_box(
            &frame,
        ))?);
        Ok(())
    })
}

fn benchmark_replay_store() -> Result<(), Box<dyn std::error::Error>> {
    use appcore_peer_rpc::{PeerNonceStore, ReplayStore};

    measure(REPLAY_CASE, 1, || {
        let policy = appcore_peer_rpc::ReplayStoreConfig::new(10_000, 60_000, 1_000)?;
        let store = appcore_peer_rpc::BoundedReplayStore::new(policy);
        for index in 0..10_000 {
            let nonce = format!("benchmark-nonce-{index:05}");
            store.check_and_record(&nonce, 60_001, 1)?;
        }
        let memory = store.memory_metrics();
        assert_eq!(store.metrics().entries, 10_000);
        assert!(memory.used_bytes <= memory.max_bytes);
        black_box(store);
        Ok(())
    })
}

fn benchmark_hash() -> Result<(), Box<dyn std::error::Error>> {
    let payload = [0x5a; 1_024];
    measure(HASH_CASE, 50_000, || {
        black_box(appcore_peer_rpc::payload_hash(black_box(&payload)));
        Ok(())
    })
}

fn benchmark_nonce_state() -> Result<(), Box<dyn std::error::Error>> {
    let root = benchmark_root();
    if root.exists() {
        std::fs::remove_dir_all(&root)?;
    }
    let path = root.join("security/nonces.json");
    let store = appcore_peer_rpc::FilePeerNonceStore::open(&path)?;
    drop(store);
    write_nonce_fixture(&path)?;
    let result = measure(NONCE_CASE, 10, || {
        black_box(appcore_peer_rpc::FilePeerNonceStore::open(&path)?);
        Ok(())
    });
    std::fs::remove_dir_all(root)?;
    result
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
        "appcore-peer-rpc::{case_name} iterations={iterations} total_ns={total_ns} ns_per_iter={:.2}",
        total_ns as f64 / iterations as f64
    );
    Ok(())
}

fn write_nonce_fixture(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
    file.write_all(b"{\"format\":\"appcore-peer-nonce-v1\",\"entries\":{")?;
    for index in 0..65_536u64 {
        if index > 0 {
            file.write_all(b",")?;
        }
        write!(file, "\"nonce-{index:05}\":{}", index + 1)?;
    }
    file.write_all(b"}}")?;
    file.flush()?;
    set_private_path(path)?;
    Ok(())
}

#[cfg(unix)]
fn set_private_path(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_private_path(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

fn benchmark_root() -> PathBuf {
    std::env::temp_dir().join(format!("appcore-peer-rpc-benchmark-{}", std::process::id()))
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
