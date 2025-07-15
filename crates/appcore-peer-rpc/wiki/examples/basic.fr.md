# Replay store borne minimal

[English](basic.en.md) | [Português](basic.pt.md) |
[Exemple intermediaire](intermediate.fr.md) | [Guide](../guide.fr.md)

Enregistrez une identite a usage unique avec des limites explicites de
capacite, TTL et nettoyage.

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

Un store rempli d'entrees vivantes refuse les nouvelles requetes au lieu
d'evincer une preuve de replay encore valide.
