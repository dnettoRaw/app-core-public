# Validate a peer command envelope

[Português](intermediate.pt.md) | [Français](intermediate.fr.md) |
[Minimal example](basic.en.md) | [Guide](../guide.en.md)

Validate target identity, tenant, cluster, protocol, time window, payload hash
and nonce replay before dispatching an authenticated peer command.

```rust
use appcore_core::{CapabilityName, ClusterId, CoreId, TenantId};
use appcore_peer_rpc::{
    BoundedReplayStore, PeerRpcEnvelope, PeerRpcError, PeerRpcValidationConfig,
    PeerRpcValidator, ReplayStoreConfig,
};
use std::sync::Arc;

fn main() -> Result<(), PeerRpcError> {
    let tenant_id = TenantId::new("tenant-acme")
        .map_err(|_| PeerRpcError::InvalidEnvelope("tenant".to_string()))?;
    let cluster_id = ClusterId::new("cluster-eu")
        .map_err(|_| PeerRpcError::InvalidEnvelope("cluster".to_string()))?;
    let target_core_id = CoreId::new("core-london")
        .map_err(|_| PeerRpcError::InvalidEnvelope("target".to_string()))?;
    let replay = Arc::new(BoundedReplayStore::new(ReplayStoreConfig::new(
        10_000, 60_000, 1_000,
    )?));
    let validator = PeerRpcValidator::new(PeerRpcValidationConfig {
        local_tenant_id: tenant_id.clone(),
        local_cluster_id: cluster_id.clone(),
        local_core_id: target_core_id.clone(),
        max_payload_bytes: 64 * 1024,
        nonce_window_ms: 30_000,
    })
    .with_nonce_store(replay);
    let envelope = PeerRpcEnvelope::new(
        "request-42",
        "trace-42",
        CoreId::new("core-paris")
            .map_err(|_| PeerRpcError::InvalidEnvelope("source".to_string()))?,
        target_core_id,
        tenant_id,
        cluster_id,
        1_700_000_000_000,
        1_700_000_030_000,
        "nonce-request-42",
        CapabilityName::new("document.create")
            .map_err(|_| PeerRpcError::InvalidEnvelope("capability".to_string()))?,
        br#"{"title":"Runtime notes"}"#.to_vec(),
        Some("create-document-42".to_string()),
        None,
    );

    validator.validate(&envelope, 1_700_000_010_000)?;
    assert_eq!(
        validator.validate(&envelope, 1_700_000_011_000),
        Err(PeerRpcError::NonceReplay)
    );
    Ok(())
}
```

This validator does not replace bearer authentication. The host must first
authenticate the signed request, then apply validation and authorization before
dispatch.

## Explicit post-1.0 V2 streaming

After the deployment explicitly enables V2 on the host, an existing
`PeerRpcClient` can move file-backed data without building one aggregate
request or response `Vec`:

```rust
use appcore_core::{CapabilityName, CoreId};
use appcore_peer_rpc::PeerRpcStreamRequestV2;
use std::fs::File;

let source = File::open("request.bin")?;
let bytes = source.metadata()?.len();
let request = PeerRpcStreamRequestV2::new(
    "request-stream-42",
    CoreId::new("core-london")?,
    CapabilityName::new("runtime.snapshot")?,
    bytes,
    None,
    None,
);
let response = File::create("response.bin")?;
let response = client.query_stream_v2(peer_url, request, source, response)?;
```

JSON is the default. To use native binary chunk bytes, the host must add
`with_v2_binary_codec()` and the client must opt in before the call:

```rust
use appcore_peer_rpc::v2::PeerRpcStreamCodecV2;

let client = client.with_stream_codec_v2(PeerRpcStreamCodecV2::Binary);
```

An unavailable binary route fails the operation; it is never retried as JSON.

Commands use `command_stream_v2` and require an idempotency key. Neither method
retries an ambiguous frame; cancellation is best effort and the declared
deadline removes unreachable partial state.

If the host rejects a frame before ambiguous acceptance, inspect the validated
typed error rather than its message:

```rust
use appcore_peer_rpc::PeerRpcStreamClientErrorV2;

if let Err(PeerRpcStreamClientErrorV2::Remote(error)) = result {
    if error.retryable {
        schedule_bounded_operation_retry(error.retry_after_ms);
    }
}
```

The operation-level retry must still be idempotent. Unknown or contradictory
remote metadata never enters this branch.
