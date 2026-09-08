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
    ApplicationId, DeploymentManifestV1, InstallationId, NetworkConfig, ProviderConfig, ProviderId,
    RuntimeMode, SecretRef,
};
use appcore_provider::{
    ProviderContext, ProviderError, ProviderFactory, ProviderResult, ResolvedSecret, SecretProvider,
};

struct TestSecrets;

impl SecretProvider for TestSecrets {
    fn resolve(&self, reference: &SecretRef) -> ProviderResult<ResolvedSecret> {
        assert_eq!(reference.as_str(), "env:APPCORE_CONTROL_TOKEN");
        ResolvedSecret::new("test-token")
    }
}

fn context() -> ProviderContext {
    let manifest = DeploymentManifestV1::builder(
        InstallationId::new("install-a").unwrap(),
        ApplicationId::new("app-a").unwrap(),
        RuntimeMode::Cluster,
        ProviderConfig::new(ProviderId::new("file").unwrap()),
        NetworkConfig::new(
            ProviderId::new("https").unwrap(),
            ProviderId::new("https").unwrap(),
        ),
    )
    .with_control_plane(ProviderConfig::new(
        ProviderId::new(VERCEL_NEON_PROVIDER_ID).unwrap(),
    ))
    .with_peer_discovery(ProviderConfig::new(
        ProviderId::new("control-plane").unwrap(),
    ))
    .build()
    .unwrap();
    ProviderContext::from_manifest(&manifest)
}

#[test]
fn creates_client_without_neon_credentials_on_the_runtime() {
    let config = ProviderConfig::new(ProviderId::new(VERCEL_NEON_PROVIDER_ID).unwrap())
        .with_endpoint("https://control.example.test")
        .unwrap()
        .with_secret_ref(
            AUTH_TOKEN_SECRET,
            SecretRef::new("env:APPCORE_CONTROL_TOKEN").unwrap(),
        )
        .unwrap();
    assert!(VercelNeonControlPlaneFactory
        .create(&config, &context(), &TestSecrets)
        .is_ok());
    assert!(!config.secret_refs().contains_key("database_url"));
}

#[test]
fn rejects_plain_http_and_missing_token_reference() {
    let plain = ProviderConfig::new(ProviderId::new(VERCEL_NEON_PROVIDER_ID).unwrap())
        .with_endpoint("http://control.example.test")
        .unwrap();
    assert!(matches!(
        VercelNeonControlPlaneFactory.create(&plain, &context(), &TestSecrets),
        Err(ProviderError::InvalidConfiguration(_))
    ));
}

#[test]
fn rejects_excessive_retry_settings_before_secret_resolution() {
    struct NoSecrets;
    impl SecretProvider for NoSecrets {
        fn resolve(&self, _: &SecretRef) -> ProviderResult<ResolvedSecret> {
            panic!("invalid settings must not resolve secrets");
        }
    }
    for (name, value) in [
        ("max_attempts", "17"),
        ("timeout_ms", "30001"),
        ("initial_backoff_ms", "30001"),
        ("max_backoff_ms", "30001"),
        ("max_attempts", "0"),
        ("timeout_ms", "18446744073709551615"),
        ("timeout_ms", "invalid"),
        ("initial_backoff_ms", "1001"),
    ] {
        let config = ProviderConfig::new(ProviderId::new(VERCEL_NEON_PROVIDER_ID).unwrap())
            .with_endpoint("https://control.example.test")
            .unwrap()
            .with_secret_ref(
                AUTH_TOKEN_SECRET,
                SecretRef::new("env:APPCORE_CONTROL_TOKEN").unwrap(),
            )
            .unwrap()
            .with_setting(name, value)
            .unwrap();
        assert!(matches!(
            VercelNeonControlPlaneFactory.create(&config, &context(), &NoSecrets),
            Err(ProviderError::InvalidConfiguration(_))
        ));
    }
}

#[test]
fn retry_work_budget_accepts_exact_limit_and_rejects_one_more_millisecond() {
    let config = ProviderConfig::new(ProviderId::new(VERCEL_NEON_PROVIDER_ID).unwrap())
        .with_endpoint("https://control.example.test")
        .unwrap()
        .with_secret_ref(
            AUTH_TOKEN_SECRET,
            SecretRef::new("env:APPCORE_CONTROL_TOKEN").unwrap(),
        )
        .unwrap()
        .with_setting("max_attempts", "4")
        .unwrap()
        .with_setting("timeout_ms", "29250")
        .unwrap();
    assert!(VercelNeonControlPlaneFactory
        .create(&config, &context(), &TestSecrets)
        .is_ok());
    let excessive = config.with_setting("timeout_ms", "29251").unwrap();
    assert!(matches!(
        VercelNeonControlPlaneFactory.create(&excessive, &context(), &TestSecrets),
        Err(ProviderError::InvalidConfiguration(_))
    ));
}
