// =============================================================================
//        #######
//     ###       ###     F: registry_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/03 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/03 00:00:00 by dnettoRaw
//      ###########      S: 1.0.6-rc
// =============================================================================
// appcore-norm: test

use super::*;
use appcore_contracts::InstallationId;
use appcore_types::{CoreId, TenantId};

#[test]
fn capability_names_are_interned_across_workers_and_released() {
    let mut registry = CapabilityRegistry::new();
    let first = worker("first");
    let second = worker("second");
    let query = capability("runtime.query");
    let command = capability("runtime.command");
    registry.register(
        first.clone(),
        vec![query.clone(), command.clone(), query.clone()],
    );
    registry.register(second.clone(), vec![query.clone()]);

    let first_names = registry.worker_to_capabilities.get(&first).unwrap();
    let second_names = registry.worker_to_capabilities.get(&second).unwrap();
    let first_query = first_names
        .iter()
        .find(|name| name.as_str() == query.as_str())
        .unwrap();
    assert!(Arc::ptr_eq(first_query, &second_names[0]));
    assert_eq!(
        registry
            .capabilities_for_iter(&first)
            .map(CapabilityName::as_str)
            .collect::<Vec<_>>(),
        ["runtime.command", "runtime.query"]
    );
    assert_eq!(
        registry.stats(),
        CapabilityRegistryStats {
            unique_capabilities: 2,
            workers: 2,
            advertisements: 3,
            capability_name_bytes: "runtime.command".len() + "runtime.query".len(),
        }
    );

    registry.deregister(&first);
    assert!(registry.resolve(&command).is_none());
    assert_eq!(registry.resolve(&query).map(HashSet::len), Some(1));
    registry.deregister(&second);
    assert_eq!(registry.stats(), CapabilityRegistryStats::default());
}

#[test]
fn clone_shares_interned_names_without_coupling_mutation() {
    let mut registry = CapabilityRegistry::new();
    let worker = worker("clone");
    registry.register(worker.clone(), vec![capability("runtime.query")]);
    let mut cloned = registry.clone();

    let original_name = &registry.worker_to_capabilities.get(&worker).unwrap()[0];
    let cloned_name = &cloned.worker_to_capabilities.get(&worker).unwrap()[0];
    assert!(Arc::ptr_eq(original_name, cloned_name));
    cloned.deregister(&worker);
    assert_eq!(registry.stats().workers, 1);
    assert_eq!(cloned.stats(), CapabilityRegistryStats::default());
}

fn worker(suffix: &str) -> WorkerConnectionKey {
    WorkerConnectionKey {
        tenant_id: TenantId::new("tenant-registry").unwrap(),
        installation_id: InstallationId::new(format!("installation-{suffix}")).unwrap(),
        core_id: CoreId::new(format!("core-{suffix}")).unwrap(),
    }
}

fn capability(value: &str) -> CapabilityName {
    CapabilityName::new(value).unwrap()
}
