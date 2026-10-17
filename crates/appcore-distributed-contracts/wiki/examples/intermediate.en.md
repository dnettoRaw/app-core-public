# Validate and deduplicate opaque transport

[Português](intermediate.pt.md) | [Français](intermediate.fr.md) |
[Minimal example](basic.en.md) | [Guide](../guide.en.md)

A gateway can validate routing metadata without opening encrypted application
content. Apply the bounded deduplicator only after policy validation succeeds.

```rust
use appcore_distributed_contracts::{
    OpaqueContentEnvelopeV1, OpaqueEnvelopeDecision,
    OpaqueEnvelopeDeduplicator, OpaqueEnvelopePolicy,
};
use appcore_types::{CapabilityName, InstanceId, RuntimeResult, TenantId};

fn main() -> RuntimeResult<()> {
    let capability = CapabilityName::new("sync.consume")?;
    let policy = OpaqueEnvelopePolicy {
        accepted_envelope_versions: vec![1],
        max_payload_bytes: 64 * 1024,
        accepted_capabilities: vec![capability.clone()],
    };
    let envelope = OpaqueContentEnvelopeV1::new(
        TenantId::new("tenant-acme")?,
        InstanceId::new("notes-eu-1")?,
        "message-00042",
        Some("sync-cycle-7".into()),
        capability,
        vec![0x44, 0x4e, 0x54, 0x01],
        1,
        1_700_000_000_000,
        1_700_000_060_000,
        Some(5),
    );

    if envelope.validate_transport(&policy, 1_700_000_010_000)
        != OpaqueEnvelopeDecision::Accepted
    {
        return Err(appcore_types::RuntimeError::InvalidRequest {
            kind: "opaque envelope",
            reason: "transport policy rejected",
        });
    }

    let mut deduplicator = OpaqueEnvelopeDeduplicator::new(10_000);
    assert_eq!(
        deduplicator.accept(&envelope.message_id),
        OpaqueEnvelopeDecision::Accepted
    );
    assert_eq!(
        deduplicator.accept(&envelope.message_id),
        OpaqueEnvelopeDecision::Duplicate
    );
    Ok(())
}
```

The transport sees only bounded metadata and opaque bytes. Decryption and
application schema validation remain with the destination consumer.

Additional bounded contract example; execute in a Result-returning context with explicitly selected dependencies.

```rust
use appcore_distributed_contracts::peer_rpc::v2::{PeerRpcDiagnosticV2, PeerRpcWireErrorV2, PeerRpcWireErrorCodeV2};
use appcore_types::CoreId;
let wire = PeerRpcWireErrorV2::controlled(Some("request-a".into()), None, PeerRpcWireErrorCodeV2::Stale);
let diagnostic = PeerRpcDiagnosticV2::from_wire(CoreId::new("peer-a")?, wire)?;
assert!(!diagnostic.error().retryable);
```


Exact authority composition (host supplies clock, private storage, TLS and issuer ACL):

```rust
use appcore_distributed_contracts::control_plane::lease_authority::ServiceLeaseAuthority;
use appcore_distributed_contracts::control_plane::v1::{ControlPlaneResult, ServiceLeaderLease};
async fn require_current(authority: &dyn ServiceLeaseAuthority, lease: &ServiceLeaderLease)
    -> ControlPlaneResult<()>
{
    // Observation only; the owning storage still fences its atomic commit.
    authority.check(lease).await
}
```
