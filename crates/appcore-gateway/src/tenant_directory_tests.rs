// =============================================================================
//        #######
//     ###       ###     F: tenant_directory_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/02 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/02 00:00:00 by dnettoRaw
//      ###########      S: 1.0.6-rc
// =============================================================================

//! Tests bounded copy-on-write snapshots of tenant partitions.

use super::*;

#[test]
fn snapshot_is_stable_and_shares_existing_tenant_partitions() {
    let directory = TenantDirectory::new();
    let first_id = TenantId::new("tenant-directory-first").unwrap();
    let second_id = (0..1_000)
        .map(|index| TenantId::new(format!("tenant-directory-cow-{index}")))
        .map(Result::unwrap)
        .find(|candidate| {
            candidate != &first_id
                && std::ptr::eq(directory.shard(&first_id), directory.shard(candidate))
        })
        .unwrap();
    let first = directory.get_or_insert(&first_id).unwrap();
    let stable = directory.snapshot();

    directory.get_or_insert(&second_id).unwrap();

    let stable_entries = stable.iter().collect::<Vec<_>>();
    assert_eq!(stable_entries.len(), 1);
    assert_eq!(stable_entries[0].0, &first_id);
    assert!(Arc::ptr_eq(stable_entries[0].1, &first));
    assert_eq!(directory.snapshot().iter().count(), 2);
}

#[test]
fn repeated_snapshot_does_not_clone_tenant_state() {
    let directory = TenantDirectory::new();
    let tenant_id = TenantId::new("tenant-directory-shared").unwrap();
    let partition = directory.get_or_insert(&tenant_id).unwrap();

    for _ in 0..100 {
        let snapshot = directory.snapshot();
        let (_, retained) = snapshot.iter().next().unwrap();
        assert!(Arc::ptr_eq(retained, &partition));
    }
    assert_eq!(directory.connection_count(), 0);
}
