// =============================================================================
//        #######
//     ###       ###     F: tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;
use appcore_contracts::{
    ApplicationId, CapabilityId, CoreId, DeploymentManifestV1, InstallationId, JobId,
    NetworkConfig, ProviderConfig, ProviderId, RuntimeMode, SecretRef,
};

struct NoSecrets;

impl SecretProvider for NoSecrets {
    fn resolve(&self, _reference: &SecretRef) -> ProviderResult<ResolvedSecret> {
        Err(ProviderError::SecretUnavailable("disabled".to_string()))
    }
}

struct TextFactory;

impl ProviderFactory<String> for TextFactory {
    fn role(&self) -> ProviderRole {
        ProviderRole::Storage
    }

    fn provider_id(&self) -> &'static str {
        "test"
    }

    fn create(
        &self,
        config: &ProviderConfig,
        _context: &ProviderContext,
        _secrets: &dyn SecretProvider,
    ) -> ProviderResult<String> {
        Ok(config.provider_id().as_str().to_string())
    }
}

#[test]
fn registry_requires_an_explicit_factory() {
    let manifest = DeploymentManifestV1::builder(
        InstallationId::new("install-a").unwrap(),
        ApplicationId::new("app-a").unwrap(),
        RuntimeMode::Standalone,
        ProviderConfig::new(ProviderId::new("test").unwrap()),
        NetworkConfig::new(
            ProviderId::new("http").unwrap(),
            ProviderId::new("http").unwrap(),
        ),
    )
    .build()
    .unwrap();
    let plan = DeploymentProviderPlan::from_manifest(&manifest);
    let mut registry = ProviderRegistry::new();
    assert!(matches!(
        registry.create(
            ProviderRole::Storage,
            plan.storage(),
            plan.context(),
            &NoSecrets
        ),
        Err(ProviderError::Unavailable { .. })
    ));
    registry.register(TextFactory).unwrap();
    assert_eq!(
        registry
            .create(
                ProviderRole::Storage,
                plan.storage(),
                plan.context(),
                &NoSecrets
            )
            .unwrap(),
        "test"
    );
}

#[test]
fn provider_plan_keeps_each_infrastructure_role_separate() {
    let manifest = DeploymentManifestV1::builder(
        InstallationId::new("install-a").unwrap(),
        ApplicationId::new("app-a").unwrap(),
        RuntimeMode::Cluster,
        ProviderConfig::new(ProviderId::new("storage").unwrap()),
        NetworkConfig::new(
            ProviderId::new("peer").unwrap(),
            ProviderId::new("command").unwrap(),
        ),
    )
    .with_control_plane(ProviderConfig::new(ProviderId::new("control").unwrap()))
    .with_coordination_store(ProviderConfig::new(
        ProviderId::new("coordination").unwrap(),
    ))
    .with_secret_provider(ProviderConfig::new(ProviderId::new("environment").unwrap()))
    .with_job_provider(ProviderConfig::new(ProviderId::new("jobs").unwrap()))
    .with_peer_discovery(ProviderConfig::new(ProviderId::new("discovery").unwrap()))
    .build()
    .unwrap();

    let plan = DeploymentProviderPlan::from_manifest(&manifest);
    assert_eq!(
        plan.coordination_store().unwrap().provider_id().as_str(),
        "coordination"
    );
    assert_eq!(
        plan.secret_provider().unwrap().provider_id().as_str(),
        "environment"
    );
    assert_eq!(plan.job_provider().unwrap().provider_id().as_str(), "jobs");
}

struct CompatibleCoordinationStore;

impl CoordinationStoreProvider for CompatibleCoordinationStore {
    fn schema_version(&self) -> ProviderResult<u64> {
        Ok(COORDINATION_SCHEMA_VERSION)
    }

    fn health(&self) -> ProviderResult<()> {
        Ok(())
    }
}

#[test]
fn coordination_store_checks_the_runtime_schema_version() {
    assert!(CompatibleCoordinationStore.ensure_compatible().is_ok());
}

#[test]
fn in_memory_coordination_store_enforces_schema_and_health() {
    let store = InMemoryCoordinationStore::default();
    assert!(store.ensure_compatible().is_ok());

    let old = InMemoryCoordinationStore::with_schema_version(1);
    assert!(old.ensure_compatible().is_err());

    store.set_healthy(false);
    assert!(store.ensure_compatible().is_err());
}

#[test]
fn file_coordination_store_migrates_backs_up_and_restores_v2() {
    let root = std::env::temp_dir().join(format!(
        "appcore-coordination-provider-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("coordination-schema.meta"),
        "format=appcore.coordination-store.v1\nversion=1\ntables=audit\n",
    )
    .unwrap();

    let store = FileCoordinationStore::open(&root).unwrap();
    assert_eq!(store.schema_version().unwrap(), 2);
    assert!(store.ensure_compatible().is_ok());

    let backup = root.join("schema.backup");
    store.backup_to(&backup).unwrap();
    std::fs::write(
        root.join("coordination-schema.meta"),
        "format=appcore.coordination-store.v1\nversion=2\ntables=customers\n",
    )
    .unwrap();
    assert!(store.health().is_err());
    store.restore_from(&backup).unwrap();
    assert!(store.health().is_ok());

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn job_contract_uses_opaque_references_and_fenced_leases() {
    let job = JobSpec::new(
        JobId::new("job-a").unwrap(),
        CapabilityId::new("document.extract").unwrap(),
        "provider:payload-a",
        100,
        3,
    )
    .unwrap();
    let lease = JobLease::new(
        job.job_id().clone(),
        CoreId::new("core-a").unwrap(),
        1,
        1_000,
    )
    .unwrap();

    assert_eq!(job.payload_reference(), "provider:payload-a");
    assert!(!format!("{job:?}").contains("payload-a"));
    assert_eq!(lease.epoch(), 1);
    assert!(JobSpec::new(
        JobId::new("job-b").unwrap(),
        CapabilityId::new("document.extract").unwrap(),
        "",
        100,
        3,
    )
    .is_err());
}

#[test]
fn job_provider_defaults_to_fenced_compare_and_swap() {
    struct ContractProvider;

    impl JobProvider for ContractProvider {
        fn submit(&self, _job: JobSpec) -> ProviderResult<()> {
            Ok(())
        }

        fn claim(
            &self,
            _capability: &CapabilityId,
            _holder_core_id: &CoreId,
            _now_ms: u64,
            _lease_duration_ms: u64,
        ) -> ProviderResult<Option<JobLease>> {
            Ok(None)
        }

        fn complete(&self, _lease: &JobLease, _completion: JobCompletion) -> ProviderResult<()> {
            Ok(())
        }
    }

    assert_eq!(
        ContractProvider.atomicity(),
        JobAtomicity::FencedCompareAndSwap
    );
}
