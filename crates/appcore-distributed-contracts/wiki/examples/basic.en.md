# Minimal peer RPC envelope

[Português](basic.pt.md) | [Français](basic.fr.md) |
[Intermediate example](intermediate.en.md) | [Guide](../guide.en.md)

Build one versioned peer request. The constructor binds the payload hash, so a
consumer can detect payload changes before dispatch.

```rust
use appcore_distributed_contracts::PeerRpcEnvelope;
use appcore_types::{CapabilityName, ClusterId, CoreId, RuntimeResult, TenantId};

fn main() -> RuntimeResult<()> {
    let request = PeerRpcEnvelope::new(
        "request-42",
        "trace-42",
        CoreId::new("core-paris")?,
        CoreId::new("core-london")?,
        TenantId::new("tenant-acme")?,
        ClusterId::new("cluster-eu")?,
        1_700_000_000_000,
        1_700_000_030_000,
        "single-use-nonce",
        CapabilityName::new("notes.read")?,
        br#"{"note_id":"42"}"#.to_vec(),
        None,
        None,
    );

    println!("request={} hash={}", request.request_id, request.body_hash);
    Ok(())
}
```

The envelope is only a wire contract. Authentication, replay protection and
transport execution belong to `appcore-peer-rpc`.
