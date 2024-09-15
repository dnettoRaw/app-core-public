# appcore-sync

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Conservative leader-to-follower replication with versioned wire, log,
snapshots, checkpoints, outbox, receiver and transport contracts.

Identity, protocol, sequence and hash-chain validation are mandatory. This
crate is not RAFT, multi-master consensus or a domain conflict resolver.

File logs, snapshots, checkpoints and outbox records are versioned and bounded.
The receiver validates the complete incoming batch, sequence range and record
sizes before mutating the replication log or checkpoint.

```bash
cargo test -p appcore-sync
```
