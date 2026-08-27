// =============================================================================
//        #######
//     ###       ###     F: resolver_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.5.0-alpha.1
// =============================================================================
// appcore-norm: test

use super::*;
use crate::connection::CONNECTION_BUFFER_CAPACITY;
use appcore_contracts::InstallationId;
use appcore_types::{CoreId, TenantId};
use axum::extract::ws::Message;
use tokio::sync::mpsc;

const NOW_MS: u64 = 100_000;

struct Fixture {
    tenant: TenantState,
    capability: CapabilityName,
    _receivers: Vec<mpsc::Receiver<Message>>,
}

fn tenant_with_workers(tenant_name: &str, heartbeats: &[u64]) -> Fixture {
    let tenant_id = TenantId::new(tenant_name).unwrap();
    let capability = CapabilityName::new("runtime.selection").unwrap();
    let mut tenant = TenantState::new(tenant_id.clone());
    let mut receivers = Vec::new();
    for (index, heartbeat) in heartbeats.iter().enumerate() {
        let (sender, receiver) = mpsc::channel(CONNECTION_BUFFER_CAPACITY);
        let worker = crate::WorkerConnection::new(
            WorkerConnectionKey {
                tenant_id: tenant_id.clone(),
                installation_id: InstallationId::new(format!("installation-{index:02}")).unwrap(),
                core_id: CoreId::new(format!("core-{index:02}")).unwrap(),
            },
            sender,
            *heartbeat,
        );
        tenant.add_worker(worker, vec![capability.clone()]).unwrap();
        receivers.push(receiver);
    }
    Fixture {
        tenant,
        capability,
        _receivers: receivers,
    }
}

fn input() -> WorkerSelectionInput<'static> {
    WorkerSelectionInput::new(NOW_MS, Duration::from_secs(10))
}

#[test]
fn first_available_is_stable_independent_of_hash_iteration() {
    let fixture = tenant_with_workers("tenant-first", &[NOW_MS; 4]);
    let resolver = CapabilityResolver::new();

    for _ in 0..32 {
        assert_eq!(
            resolver
                .select(&fixture.capability, &fixture.tenant, input())
                .unwrap()
                .core_id
                .as_str(),
            "core-00"
        );
    }
    assert_eq!(resolver.policy(), SelectionPolicy::FirstAvailable);
}

#[test]
fn round_robin_distribution_is_exact_and_repeatable() {
    let fixture = tenant_with_workers("tenant-round-robin", &[NOW_MS; 4]);
    let resolver = CapabilityResolver::with_policy(SelectionPolicy::RoundRobin);
    let selected = (0..12)
        .map(|_| {
            resolver
                .select(&fixture.capability, &fixture.tenant, input())
                .unwrap()
                .core_id
                .as_str()
                .to_string()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        selected,
        [
            "core-00", "core-01", "core-02", "core-03", "core-00", "core-01", "core-02", "core-03",
            "core-00", "core-01", "core-02", "core-03"
        ]
    );
}

#[test]
fn least_inflight_uses_queue_depth_and_stable_identity_as_ties() {
    let fixture = tenant_with_workers("tenant-least", &[NOW_MS; 3]);
    let worker_zero = fixture
        .tenant
        .get_worker(
            &InstallationId::new("installation-00").unwrap(),
            &CoreId::new("core-00").unwrap(),
        )
        .unwrap();
    let worker_one = fixture
        .tenant
        .get_worker(
            &InstallationId::new("installation-01").unwrap(),
            &CoreId::new("core-01").unwrap(),
        )
        .unwrap();
    let _zero_permits = (0..2)
        .map(|_| worker_zero.try_admit_route(8).unwrap())
        .collect::<Vec<_>>();
    let _one_permit = worker_one.try_admit_route(8).unwrap();
    let resolver = CapabilityResolver::with_policy(SelectionPolicy::LeastInflight);

    assert_eq!(
        resolver
            .select(&fixture.capability, &fixture.tenant, input())
            .unwrap()
            .core_id
            .as_str(),
        "core-02"
    );
}

#[test]
fn health_weighting_excludes_stale_and_prefers_fresh_workers() {
    let fixture = tenant_with_workers("tenant-health", &[NOW_MS, NOW_MS - 8_000, NOW_MS - 10_001]);
    let resolver = CapabilityResolver::with_policy(SelectionPolicy::HealthWeighted);
    let mut fresh = 0;
    let mut aging = 0;
    let mut stale = 0;
    for _ in 0..100 {
        match resolver
            .select(&fixture.capability, &fixture.tenant, input())
            .unwrap()
            .core_id
            .as_str()
        {
            "core-00" => fresh += 1,
            "core-01" => aging += 1,
            _ => stale += 1,
        }
    }

    assert!(fresh > aging);
    assert!(aging > 0);
    assert_eq!(stale, 0);
}

#[test]
fn affinity_is_stable_bounded_and_tenant_local() {
    let fixture_a = tenant_with_workers("tenant-affinity-a", &[NOW_MS; 4]);
    let fixture_b = tenant_with_workers("tenant-affinity-b", &[NOW_MS; 4]);
    let resolver = CapabilityResolver::with_policy(SelectionPolicy::Affinity);
    let affinity_input = input().with_affinity("device-session-a");
    let selected = resolver
        .select(&fixture_a.capability, &fixture_a.tenant, affinity_input)
        .unwrap();

    for index in 0..2_048 {
        let affinity = format!("bounded-affinity-{index}");
        let candidate = resolver
            .select(
                &fixture_a.capability,
                &fixture_a.tenant,
                input().with_affinity(&affinity),
            )
            .unwrap();
        assert_eq!(candidate.tenant_id, fixture_a.tenant.tenant_id);
    }
    assert_eq!(
        resolver
            .select(&fixture_a.capability, &fixture_a.tenant, affinity_input)
            .unwrap(),
        selected
    );
    assert_eq!(
        resolver
            .select(&fixture_b.capability, &fixture_b.tenant, affinity_input)
            .unwrap()
            .tenant_id,
        fixture_b.tenant.tenant_id
    );
    assert_eq!(
        resolver.select(
            &fixture_a.capability,
            &fixture_a.tenant,
            input().with_affinity("")
        ),
        Err(WorkerSelectionError::InvalidAffinity)
    );
}

#[test]
fn health_and_inflight_limits_fail_closed_with_typed_reasons() {
    let stale = tenant_with_workers("tenant-stale", &[NOW_MS - 20_000; 2]);
    let resolver = CapabilityResolver::with_policy(SelectionPolicy::LeastInflight);
    assert_eq!(
        resolver.select(&stale.capability, &stale.tenant, input()),
        Err(WorkerSelectionError::NoHealthyWorker)
    );

    let full = tenant_with_workers("tenant-full", &[NOW_MS; 2]);
    let _permits = full
        .tenant
        .workers
        .values()
        .map(|worker| worker.try_admit_route(1).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        resolver.select(&full.capability, &full.tenant, input().with_max_inflight(1)),
        Err(WorkerSelectionError::AtCapacity)
    );
    assert_eq!(
        resolver.select(&full.capability, &full.tenant, input().with_max_inflight(0)),
        Err(WorkerSelectionError::InvalidLimits)
    );
}
