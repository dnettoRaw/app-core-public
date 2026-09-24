// =============================================================================
//        #######
//     ###       ###     F: fixtures.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/24 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/24 00:00:00 by dnettoRaw
//      ###########      S: 1.0.3-rc
// =============================================================================
// appcore-norm: test

use appcore_update::{
    ActivationReceiptV2, ArtifactAuthenticityVerifier, ArtifactDescriptor, ReleaseCatalog,
    UpdateError, UpdateResult,
};
use sha2::{Digest, Sha256};

struct FixtureVerifier;

impl ArtifactAuthenticityVerifier for FixtureVerifier {
    fn verify(&self, _artifact: &ArtifactDescriptor) -> UpdateResult<()> {
        Ok(())
    }
}

#[test]
fn cache_fixtures_are_bounded_and_digest_mismatched() {
    let partial = include_bytes!("../fixtures/cache/partial.bin");
    let corrupt = include_bytes!("../fixtures/cache/corrupt-object.bin");
    assert!(!partial.is_empty());
    let actual = Sha256::digest(corrupt);
    let actual = actual
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/cache/manifest.json")).unwrap();
    assert_eq!(manifest["schema_version"], 1);
    assert_ne!(actual, manifest["corrupt_object_sha256"].as_str().unwrap());
}

#[test]
fn ambiguous_catalog_fixture_is_rejected_before_selection() {
    let entries: Vec<ArtifactDescriptor> =
        serde_json::from_str(include_str!("../fixtures/catalog/ambiguous.json")).unwrap();
    let error = ReleaseCatalog::from_entries(entries, &FixtureVerifier).unwrap_err();
    assert!(
        matches!(error, UpdateError::InvalidArtifact(message) if message.contains("ambiguous"))
    );
}

#[test]
fn incomplete_receipt_fixture_hits_decode_wall() {
    let result = serde_json::from_str::<ActivationReceiptV2>(include_str!(
        "../fixtures/receipts/incomplete-v2.json"
    ));
    assert!(result.is_err());
}
