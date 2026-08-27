// =============================================================================
//        #######
//     ###       ###     F: redis_registry_conformance.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.6-rc
// =============================================================================
// appcore-norm: test

use appcore_contracts::InstallationId;
use appcore_gateway::config::{
    MAX_GATEWAY_CLIENTS_PER_TENANT, MAX_GATEWAY_PENDING_PER_TENANT, MAX_GATEWAY_WORKERS_PER_TENANT,
};
use appcore_gateway::{
    GatewayFederationUrl, GatewayInstanceLease, GatewayRegistryError, GatewayRegistryProvider,
    GatewayRequestFence, GatewaySessionRecord, GatewayWorkerRecord, GatewayWorkerRegistration,
    RedisGatewayCredential, RedisGatewayRegistryConfig, RedisGatewayRegistryProvider,
};
use appcore_types::{CapabilityName, ClusterId, CoreId, InstanceId, TenantId};
use std::time::{SystemTime, UNIX_EPOCH};
use zeroize::Zeroizing;

#[test]
#[ignore = "requires APPCORE_GATEWAY_REDIS_URL and APPCORE_GATEWAY_REDIS_CREDENTIAL"]
fn redis_provider_enforces_shared_ownership_and_fencing() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(run_conformance());
}

async fn run_conformance() {
    let Some((endpoint, credential)) = redis_environment() else {
        return;
    };
    let namespace = format!(
        "appcore-test-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    verify_schema_wall(&endpoint, &namespace, &credential).await;
    let fixture = Fixture::new(&endpoint, &namespace, &credential).await;
    let ownership = verify_ownership_and_discovery(&fixture).await;
    verify_request_fencing(&fixture, ownership).await;
    verify_outage_recovery(&fixture, &endpoint, &namespace, &credential).await;
}

fn redis_environment() -> Option<(String, String)> {
    match (
        std::env::var("APPCORE_GATEWAY_REDIS_URL"),
        std::env::var("APPCORE_GATEWAY_REDIS_CREDENTIAL"),
    ) {
        (Ok(endpoint), Ok(credential)) => Some((endpoint, credential)),
        (Err(std::env::VarError::NotPresent), Err(std::env::VarError::NotPresent)) => None,
        _ => panic!("Redis Gateway test environment is incomplete or invalid"),
    }
}

async fn verify_schema_wall(endpoint: &str, namespace: &str, credential: &str) {
    let isolated_namespace = format!("{namespace}-wall");
    let mut connection = authenticated_connection(endpoint, credential).await;
    redis::cmd("SET")
        .arg(format!("{isolated_namespace}:schema"))
        .arg("appcore.gateway.ha.removed")
        .query_async::<()>(&mut connection)
        .await
        .unwrap();
    let error = RedisGatewayRegistryProvider::connect(
        RedisGatewayRegistryConfig::new(endpoint, isolated_namespace, 2_000, 8).unwrap(),
        credential_owner(credential),
    )
    .await
    .unwrap_err();
    assert_eq!(error, GatewayRegistryError::UnsupportedSchema);

    let record_namespace = format!("{namespace}-record");
    let provider = connect(endpoint, &record_namespace, credential).await;
    let tenant = TenantId::new("tenant-schema").unwrap();
    let cluster = ClusterId::new("cluster-schema").unwrap();
    let instance = InstanceId::new("gateway-schema").unwrap();
    let lease = provider
        .acquire_instance(
            &tenant,
            &cluster,
            &instance,
            &GatewayFederationUrl::new("https://schema.example.com").unwrap(),
            10_000,
            1_000,
        )
        .await
        .unwrap();
    redis::cmd("HSET")
        .arg(format!(
            "{record_namespace}:{{{}}}:lease:{}",
            tenant.as_str(),
            instance.as_str()
        ))
        .arg("schema")
        .arg("appcore.gateway.ha.removed")
        .query_async::<()>(&mut connection)
        .await
        .unwrap();
    assert_eq!(
        provider.check_instance(&lease, 1_000).await,
        Err(GatewayRegistryError::UnsupportedSchema)
    );
}

struct Fixture {
    first: RedisGatewayRegistryProvider,
    second: RedisGatewayRegistryProvider,
    tenant: TenantId,
    other_tenant: TenantId,
    cluster: ClusterId,
    origin_id: InstanceId,
    target_id: InstanceId,
    target_url: GatewayFederationUrl,
    origin_url: GatewayFederationUrl,
    capability: CapabilityName,
    now: u64,
}

impl Fixture {
    async fn new(endpoint: &str, namespace: &str, credential: &str) -> Self {
        Self {
            first: connect(endpoint, namespace, credential).await,
            second: connect(endpoint, namespace, credential).await,
            tenant: TenantId::new("tenant-a").unwrap(),
            other_tenant: TenantId::new("tenant-b").unwrap(),
            cluster: ClusterId::new("cluster-a").unwrap(),
            origin_id: InstanceId::new("gateway-origin").unwrap(),
            target_id: InstanceId::new("gateway-target").unwrap(),
            target_url: GatewayFederationUrl::new("https://target.example.com").unwrap(),
            origin_url: GatewayFederationUrl::new("https://origin.example.com").unwrap(),
            capability: CapabilityName::new("runtime.query").unwrap(),
            now: 1_000,
        }
    }
}

struct Ownership {
    target: GatewayInstanceLease,
    origin: GatewayInstanceLease,
    worker: GatewayWorkerRecord,
}

async fn verify_ownership_and_discovery(fixture: &Fixture) -> Ownership {
    let target = fixture
        .first
        .acquire_instance(
            &fixture.tenant,
            &fixture.cluster,
            &fixture.target_id,
            &fixture.target_url,
            60_000,
            fixture.now,
        )
        .await
        .unwrap();
    assert_eq!(
        fixture
            .second
            .acquire_instance(
                &fixture.tenant,
                &fixture.cluster,
                &fixture.target_id,
                &fixture.target_url,
                60_000,
                fixture.now,
            )
            .await,
        Err(GatewayRegistryError::Conflict)
    );
    let origin = fixture
        .second
        .acquire_instance(
            &fixture.tenant,
            &fixture.cluster,
            &fixture.origin_id,
            &fixture.origin_url,
            60_000,
            fixture.now,
        )
        .await
        .unwrap();
    let worker = fixture
        .first
        .register_worker(
            &target,
            worker_registration(1, fixture.capability.clone()),
            60_000,
            fixture.now,
        )
        .await
        .unwrap();
    verify_worker_discovery(fixture, &target, &origin, &worker).await;
    Ownership {
        target,
        origin,
        worker,
    }
}

async fn verify_worker_discovery(
    fixture: &Fixture,
    target: &GatewayInstanceLease,
    origin: &GatewayInstanceLease,
    worker: &GatewayWorkerRecord,
) {
    assert_eq!(
        fixture
            .first
            .register_worker(
                target,
                worker_registration(1, fixture.capability.clone()),
                60_000,
                fixture.now,
            )
            .await,
        Err(GatewayRegistryError::StaleOwner)
    );
    let resolved = fixture
        .second
        .resolve_worker(
            &fixture.tenant,
            &fixture.cluster,
            &worker.core_id,
            fixture.now,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&resolved, worker);
    assert_eq!(
        fixture
            .second
            .resolve_capability(&fixture.tenant, &fixture.capability, 8, fixture.now)
            .await
            .unwrap(),
        vec![worker.clone()]
    );
    assert!(fixture
        .second
        .resolve_worker(
            &fixture.other_tenant,
            &fixture.cluster,
            &worker.core_id,
            fixture.now,
        )
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        fixture
            .second
            .register_worker(
                origin,
                worker_registration(1, fixture.capability.clone()),
                10_000,
                fixture.now,
            )
            .await,
        Err(GatewayRegistryError::Conflict)
    );
}

async fn verify_request_fencing(fixture: &Fixture, ownership: Ownership) {
    verify_session(fixture, &ownership.origin).await;
    let request = fixture
        .second
        .claim_request(
            &ownership.origin,
            &ownership.worker,
            "request-a",
            fixture.now + 5_000,
            fixture.now,
        )
        .await
        .unwrap();
    assert_eq!(
        fixture
            .second
            .claim_request(
                &ownership.origin,
                &ownership.worker,
                "request-a",
                fixture.now + 5_000,
                fixture.now,
            )
            .await,
        Err(GatewayRegistryError::Conflict)
    );
    let (replacement, replacement_worker) = replace_target_owner(fixture, &ownership.target).await;
    verify_replacement_fence(fixture, &ownership.origin, &request, &replacement_worker).await;
    verify_tenant_capacities(
        fixture,
        &ownership.origin,
        &replacement,
        &replacement_worker,
    )
    .await;
}

async fn replace_target_owner(
    fixture: &Fixture,
    target: &GatewayInstanceLease,
) -> (GatewayInstanceLease, GatewayWorkerRecord) {
    fixture.first.release_instance(target).await.unwrap();
    let replacement = fixture
        .first
        .acquire_instance(
            &fixture.tenant,
            &fixture.cluster,
            &fixture.target_id,
            &fixture.target_url,
            60_000,
            fixture.now + 1,
        )
        .await
        .unwrap();
    assert!(replacement.epoch() > target.epoch());
    let worker = fixture
        .first
        .register_worker(
            &replacement,
            worker_registration(2, fixture.capability.clone()),
            60_000,
            fixture.now + 1,
        )
        .await
        .unwrap();
    (replacement, worker)
}

async fn verify_replacement_fence(
    fixture: &Fixture,
    origin: &GatewayInstanceLease,
    request: &GatewayRequestFence,
    replacement_worker: &GatewayWorkerRecord,
) {
    assert_eq!(
        fixture
            .second
            .complete_request(request, fixture.now + 1)
            .await,
        Err(GatewayRegistryError::StaleOwner)
    );
    assert_eq!(
        fixture.second.check_request(request, fixture.now + 1).await,
        Err(GatewayRegistryError::StaleOwner)
    );
    fixture.second.cancel_request(request).await.unwrap();
    let next = fixture
        .second
        .claim_request(
            origin,
            replacement_worker,
            "request-b",
            fixture.now + 5_000,
            fixture.now + 1,
        )
        .await
        .unwrap();
    fixture
        .second
        .check_request(&next, fixture.now + 1)
        .await
        .unwrap();
    fixture
        .second
        .complete_request(&next, fixture.now + 1)
        .await
        .unwrap();
    assert_eq!(
        fixture
            .second
            .complete_request(&next, fixture.now + 1)
            .await,
        Err(GatewayRegistryError::Expired)
    );
}

async fn verify_session(fixture: &Fixture, origin: &GatewayInstanceLease) {
    let session =
        GatewaySessionRecord::new(origin.clone(), "session-a", fixture.now + 5_000).unwrap();
    fixture
        .second
        .register_session(origin, session.clone(), fixture.now)
        .await
        .unwrap();
    fixture
        .second
        .remove_session(origin, &session)
        .await
        .unwrap();
}

async fn verify_tenant_capacities(
    fixture: &Fixture,
    origin: &GatewayInstanceLease,
    target: &GatewayInstanceLease,
    target_worker: &GatewayWorkerRecord,
) {
    for index in 1..MAX_GATEWAY_WORKERS_PER_TENANT {
        fixture
            .first
            .register_worker(
                target,
                indexed_worker_registration(index),
                60_000,
                fixture.now + 1,
            )
            .await
            .unwrap();
    }
    assert_eq!(
        fixture
            .first
            .register_worker(
                target,
                indexed_worker_registration(MAX_GATEWAY_WORKERS_PER_TENANT),
                60_000,
                fixture.now + 1,
            )
            .await,
        Err(GatewayRegistryError::CapacityExceeded)
    );
    for index in 0..MAX_GATEWAY_CLIENTS_PER_TENANT {
        let session = GatewaySessionRecord::new(
            origin.clone(),
            format!("session-{index}"),
            fixture.now + 60_000,
        )
        .unwrap();
        fixture
            .second
            .register_session(origin, session, fixture.now + 1)
            .await
            .unwrap();
    }
    let overflow =
        GatewaySessionRecord::new(origin.clone(), "session-overflow", fixture.now + 60_000)
            .unwrap();
    assert_eq!(
        fixture
            .second
            .register_session(origin, overflow, fixture.now + 1)
            .await,
        Err(GatewayRegistryError::CapacityExceeded)
    );
    for index in 0..MAX_GATEWAY_PENDING_PER_TENANT {
        fixture
            .second
            .claim_request(
                origin,
                target_worker,
                &format!("pending-{index}"),
                fixture.now + 30_000,
                fixture.now + 1,
            )
            .await
            .unwrap();
    }
    assert_eq!(
        fixture
            .second
            .claim_request(
                origin,
                target_worker,
                "pending-overflow",
                fixture.now + 30_000,
                fixture.now + 1,
            )
            .await,
        Err(GatewayRegistryError::CapacityExceeded)
    );
}

async fn verify_outage_recovery(
    fixture: &Fixture,
    endpoint: &str,
    namespace: &str,
    credential: &str,
) {
    let outage_id = InstanceId::new("gateway-outage").unwrap();
    let outage = connect(endpoint, namespace, credential).await;
    let short_lease = outage
        .acquire_instance(
            &fixture.tenant,
            &fixture.cluster,
            &outage_id,
            &fixture.origin_url,
            100,
            fixture.now,
        )
        .await
        .unwrap();
    kill_provider_connections(endpoint, credential).await;
    assert_eq!(
        outage.check_instance(&short_lease, fixture.now).await,
        Err(GatewayRegistryError::Unavailable)
    );
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    outage.reconnect().await.unwrap();
    let recovered = outage
        .acquire_instance(
            &fixture.tenant,
            &fixture.cluster,
            &outage_id,
            &fixture.origin_url,
            10_000,
            fixture.now + 200,
        )
        .await
        .unwrap();
    assert!(recovered.epoch() > short_lease.epoch());
}

async fn connect(
    endpoint: &str,
    namespace: &str,
    credential: &str,
) -> RedisGatewayRegistryProvider {
    RedisGatewayRegistryProvider::connect(
        RedisGatewayRegistryConfig::new(endpoint, namespace, 2_000, 8).unwrap(),
        credential_owner(credential),
    )
    .await
    .unwrap()
}

fn worker_registration(generation: u64, capability: CapabilityName) -> GatewayWorkerRegistration {
    GatewayWorkerRegistration::new(
        InstallationId::new("installation-a").unwrap(),
        CoreId::new("core-a").unwrap(),
        generation,
        vec![capability],
    )
    .unwrap()
}

fn indexed_worker_registration(index: usize) -> GatewayWorkerRegistration {
    GatewayWorkerRegistration::new(
        InstallationId::new(format!("installation-{index}")).unwrap(),
        CoreId::new(format!("core-{index}")).unwrap(),
        1,
        Vec::new(),
    )
    .unwrap()
}

async fn kill_provider_connections(endpoint: &str, credential: &str) {
    let mut connection = authenticated_connection(endpoint, credential).await;
    redis::cmd("CLIENT")
        .arg("KILL")
        .arg("TYPE")
        .arg("normal")
        .arg("SKIPME")
        .arg("yes")
        .query_async::<usize>(&mut connection)
        .await
        .unwrap();
}

async fn authenticated_connection(
    endpoint: &str,
    credential: &str,
) -> redis::aio::MultiplexedConnection {
    let client = redis::Client::open(endpoint).unwrap();
    let mut connection = client.get_multiplexed_async_connection().await.unwrap();
    redis::cmd("AUTH")
        .arg(credential)
        .query_async::<()>(&mut connection)
        .await
        .unwrap();
    connection
}

fn credential_owner(credential: &str) -> RedisGatewayCredential {
    RedisGatewayCredential::new(Zeroizing::new(credential.to_string())).unwrap()
}
