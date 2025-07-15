# Minimal bounded replay store

[Português](basic.pt.md) | [Français](basic.fr.md) |
[Intermediate example](intermediate.en.md) | [Guide](../guide.en.md)

Record a single-use identity under explicit capacity, TTL and cleanup bounds.

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

A full store of live entries rejects new requests instead of evicting valid
replay evidence.
