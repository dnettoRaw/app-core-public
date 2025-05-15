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
