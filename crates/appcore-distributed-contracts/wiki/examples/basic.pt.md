# Envelope Peer RPC minimo

[English](basic.en.md) | [Français](basic.fr.md) |
[Exemplo intermediario](intermediate.pt.md) | [Guia](../guide.pt.md)

Monte uma requisicao versionada entre peers. O construtor vincula o hash do
payload, permitindo detectar alteracoes antes do dispatch.

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

O envelope e apenas o contrato de wire. Autenticacao, protecao contra replay e
execucao de transporte pertencem ao `appcore-peer-rpc`.

Exemplo adicional de contrato limitado; execute num contexto que retorna Result com dependências explicitamente selecionadas.

```rust
use appcore_distributed_contracts::peer_rpc::v2::{PeerRpcDiagnosticV2, PeerRpcWireErrorV2, PeerRpcWireErrorCodeV2};
use appcore_types::CoreId;
let wire = PeerRpcWireErrorV2::controlled(Some("request-a".into()), None, PeerRpcWireErrorCodeV2::Stale);
let diagnostic = PeerRpcDiagnosticV2::from_wire(CoreId::new("peer-a")?, wire)?;
assert!(!diagnostic.error().retryable);
```
