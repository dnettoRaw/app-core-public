// =============================================================================
//        #######
//     ###       ###     F: revision_causality.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/12 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/12 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Causal decisions preserve genuine branches and deliberate reversions.
use appcore_sync::*;
fn revision(id: &str, base: Option<&str>) -> ObjectRevisionV1 {
    ObjectRevisionV1 {
        object_id: "opaque-a".into(),
        revision_id: id.into(),
        base_revision_id: base.map(str::to_owned),
        event_id: format!("event-{id}"),
    }
}
#[test]
fn delayed_echo_and_replay_preserve_new_local_revision() {
    let a = revision("a", None);
    let b = revision("b", Some("a"));
    let c = revision("c", Some("b"));
    let chain = RevisionChainV1::new(vec![a.clone(), b.clone(), c.clone()]).unwrap();
    assert_eq!(
        decide_object_revision_v1(&c, &a, &chain).unwrap(),
        RevisionDecisionV1::AncestorEcho
    );
    assert_eq!(
        decide_object_revision_v1(&c, &c, &chain).unwrap(),
        RevisionDecisionV1::AlreadyApplied
    );
    assert_eq!(
        decide_object_revision_v1(&a, &c, &chain).unwrap(),
        RevisionDecisionV1::Applicable
    );
    let branch = revision("d", Some("b"));
    assert_eq!(
        decide_object_revision_v1(&c, &branch, &chain).unwrap(),
        RevisionDecisionV1::ConcurrentBranch
    );
    let revert = revision("revert-to-a-content", Some("c"));
    assert_eq!(
        decide_object_revision_v1(&c, &revert, &chain).unwrap(),
        RevisionDecisionV1::Applicable
    );
    assert_eq!(
        validate_checkpoint_head_v1(6, 5),
        Err(SyncError::InvalidSequence(6))
    );
}
#[test]
fn missing_history_cannot_be_inferred_and_forged_identity_is_rejected() {
    let a = revision("a", None);
    let b = revision("b", Some("a"));
    let c = revision("c", Some("b"));
    let chain = RevisionChainV1::new(vec![c.clone()]).unwrap();
    assert_eq!(
        decide_object_revision_v1(&c, &a, &chain).unwrap(),
        RevisionDecisionV1::SnapshotRequired
    );
    assert!(RevisionChainV1::new(vec![a.clone(), c]).is_err());
    let mut forged = a.clone();
    forged.base_revision_id = Some("unrelated".into());
    assert!(decide_object_revision_v1(
        &a,
        &forged,
        &RevisionChainV1::new(vec![a.clone()]).unwrap()
    )
    .is_err());
    let mut cycle = a;
    cycle.base_revision_id = Some("b".into());
    assert!(RevisionChainV1::new(vec![cycle, b]).is_err());
    assert!(RevisionChainV1::new(vec![revision("x", None); MAX_REVISION_CHAIN_V1 + 1]).is_err());
}
