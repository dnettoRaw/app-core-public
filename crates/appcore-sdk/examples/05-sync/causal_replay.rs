// =============================================================================
//        #######
//     ###       ###     F: causal_replay.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/12 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/12 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Offline causal decisions and outbox replay with explicit memory transport.
use appcore_sdk::sync::{
    decide_object_revision_v1, InMemorySyncOutbox, ObjectRevisionV1, RevisionChainV1,
    RevisionDecisionV1, SyncMessage, SyncOutbox, SyncResult,
};
fn revision(id: &str, base: Option<&str>) -> ObjectRevisionV1 {
    ObjectRevisionV1 {
        object_id: "opaque-object".into(),
        revision_id: id.into(),
        base_revision_id: base.map(str::to_owned),
        event_id: format!("event-{id}"),
    }
}
fn main() -> SyncResult<()> {
    let first = revision("revision-a", None);
    let second = revision("revision-b", Some("revision-a"));
    let chain = RevisionChainV1::new(vec![first.clone(), second.clone()])?;
    assert_eq!(
        decide_object_revision_v1(&second, &first, &chain)?,
        RevisionDecisionV1::AncestorEcho
    );
    let outbox = InMemorySyncOutbox::new();
    let batch = SyncMessage::new_simple(
        appcore_core::NodeId::new("node-a")
            .map_err(|_| appcore_sdk::sync::SyncError::InvalidPeerId)?,
        1,
        vec![b"opaque-envelope".to_vec()],
    );
    outbox.try_enqueue(batch, 8)?;
    // Explicit in-memory transport seam: fail once before acceptance, then resume.
    let mut received = Vec::new();
    let mut transport = |message: &SyncMessage, available: bool| -> SyncResult<()> {
        if !available {
            return Err(appcore_sdk::sync::SyncError::InvalidSyncMessage(
                "transport unavailable",
            ));
        }
        received.push(message.batch_id.clone());
        Ok(())
    };
    let pending = outbox
        .front()?
        .ok_or(appcore_sdk::sync::SyncError::InvalidSyncMessage(
            "missing pending batch",
        ))?;
    assert!(transport(&pending, false).is_err());
    assert_eq!(outbox.len()?, 1);
    transport(&pending, true)?;
    outbox.acknowledge_front(&pending.batch_id)?;
    assert_eq!(outbox.len()?, 0);
    assert_eq!(received.len(), 1);
    Ok(())
}
