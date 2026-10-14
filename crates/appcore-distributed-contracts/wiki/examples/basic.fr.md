# Envelope Peer RPC minimal

[English](basic.en.md) | [Português](basic.pt.md) |
[Exemple intermediaire](intermediate.fr.md) | [Guide](../guide.fr.md)

Construisez une requete versionnee entre peers. Le constructeur lie le hash du
payload afin qu'un consumer detecte toute modification avant le dispatch.

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

L'envelope reste un contrat wire. L'authentification, la protection anti-replay
et l'execution du transport appartiennent a `appcore-peer-rpc`.

Exemple supplémentaire de contrat borné ; exécutez dans un contexte retournant Result avec les dépendances explicitement choisies.

```rust
use appcore_distributed_contracts::peer_rpc::v2::{PeerRpcDiagnosticV2, PeerRpcWireErrorV2, PeerRpcWireErrorCodeV2};
use appcore_types::CoreId;
let wire = PeerRpcWireErrorV2::controlled(Some("request-a".into()), None, PeerRpcWireErrorCodeV2::Stale);
let diagnostic = PeerRpcDiagnosticV2::from_wire(CoreId::new("peer-a")?, wire)?;
assert!(!diagnostic.error().retryable);
```
