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
