# Validar um envelope de comando peer

[English](intermediate.en.md) | [Français](intermediate.fr.md) |
[Exemplo minimo](basic.pt.md) | [Guia](../guide.pt.md)

Valide destino, tenant, cluster, protocolo, janela temporal, hash do payload e
replay do nonce antes do dispatch de um comando peer autenticado.

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

Este validator nao substitui autenticacao bearer. O host deve autenticar a
requisicao assinada e depois aplicar validacao e autorizacao antes do dispatch.
