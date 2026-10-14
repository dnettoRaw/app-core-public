# Batch de replication minimal

[English](basic.en.md) | [Português](basic.pt.md) |
[Exemple intermediaire](intermediate.fr.md) | [Guide](../guide.fr.md)

Construisez un batch leader-to-follower. Le constructeur derive le nombre
d'evenements, la sequence finale et le hash d'integrite.

```rust
use appcore_core::NodeId;
use appcore_sync::SyncMessage;

fn main() -> Result<(), String> {
    let source = NodeId::new("node-paris".to_string())
        .map_err(|error| format!("{error:?}"))?;
    let batch = SyncMessage::new(
        "batch-42".to_string(),
        source,
        42,
        43,
        1_700_000_000_000,
        None,
        vec![b"document-created".to_vec(), b"document-indexed".to_vec()],
    );

    println!(
        "batch={} events={} hash={}",
        batch.batch_id, batch.event_count, batch.events_hash
    );
    Ok(())
}
```

Les evenements restent des octets applicatifs opaques. Les receivers doivent
valider sequence, compte, hash et limites avant application.

Exemple supplémentaire de contrat borné ; exécutez dans un contexte retournant Result avec les dépendances explicitement choisies.

```rust
use appcore_sync::{ObjectRevisionV1, RevisionChainV1, decide_object_revision_v1, RevisionDecisionV1};
let first = ObjectRevisionV1 { object_id: "opaque-a".into(), revision_id: "a".into(), base_revision_id: None, event_id: "event-a".into() };
let second = ObjectRevisionV1 { object_id: "opaque-a".into(), revision_id: "b".into(), base_revision_id: Some("a".into()), event_id: "event-b".into() };
let chain = RevisionChainV1::new(vec![first.clone(), second.clone()])?;
assert_eq!(decide_object_revision_v1(&second, &first, &chain)?, RevisionDecisionV1::AncestorEcho);
```
