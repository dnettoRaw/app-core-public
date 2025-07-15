# Replay store limitado minimo

[English](basic.en.md) | [Français](basic.fr.md) |
[Exemplo intermediario](intermediate.pt.md) | [Guia](../guide.pt.md)

Registre uma identidade de uso unico com limites explicitos de capacidade, TTL
e limpeza.

```rust
use appcore_peer_rpc::{
    BoundedReplayStore, PeerNonceStore, PeerRpcError, ReplayStore,
    ReplayStoreConfig,
};

fn main() -> Result<(), PeerRpcError> {
    let store = BoundedReplayStore::new(ReplayStoreConfig::new(10_000, 60_000, 1_000)?);

    store.check_and_record("nonce-request-42", 1_700_000_030_000, 1_700_000_000_000)?;
    assert_eq!(
        store.check_and_record("nonce-request-42", 1_700_000_030_000, 1_700_000_001_000),
        Err(PeerRpcError::NonceReplay)
    );
    println!("accepted={} replays={}", store.metrics().accepted, store.metrics().replays);
    Ok(())
}
```

Um store cheio de entradas vivas rejeita novas requisicoes em vez de remover
evidencia de replay ainda valida.
