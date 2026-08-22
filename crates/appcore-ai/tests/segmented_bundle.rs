// =============================================================================
//        #######
//     ###       ###     F: segmented_bundle.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use appcore_ai::{
    AiError, ArtifactBundleManifest, ArtifactDigest, ArtifactIdentity, ArtifactSegment,
    ArtifactSegmentKind, CancellationToken, CapabilityId, LocalArtifactCache, SegmentedModelReader,
};
use std::sync::Arc;

#[test]
fn reads_only_selected_verified_ranges() {
    let root = temporary_directory("segments");
    let bytes = b"tokenizer|shared-weights|expert-zero|expert-one";
    let artifact = identity(bytes);
    let cache = Arc::new(LocalArtifactCache::new(&root, 1_024).unwrap());
    cache.store(&artifact, bytes).unwrap();
    let manifest = ArtifactBundleManifest {
        artifact,
        segments: vec![
            segment("tokenizer", ArtifactSegmentKind::Tokenizer, 0, 9, bytes),
            segment("weights", ArtifactSegmentKind::Weights, 10, 14, bytes),
            segment("expert/0", ArtifactSegmentKind::Expert(0), 25, 11, bytes),
            segment("expert/1", ArtifactSegmentKind::Expert(1), 37, 10, bytes),
        ],
    };
    let reader = SegmentedModelReader::new(cache, 8, 32, 2, 32).unwrap();
    let loaded = reader
        .load(
            &manifest,
            &[
                CapabilityId::new("tokenizer").unwrap(),
                CapabilityId::new("expert/1").unwrap(),
            ],
            &CancellationToken::new(),
        )
        .unwrap();
    assert_eq!(loaded[0].bytes, b"tokenizer");
    assert_eq!(loaded[1].bytes, b"expert-one");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_corrupt_segment_identity_before_backend_load() {
    let root = temporary_directory("bad-segment");
    let bytes = b"verified-artifact";
    let artifact = identity(bytes);
    let cache = Arc::new(LocalArtifactCache::new(&root, 1_024).unwrap());
    cache.store(&artifact, bytes).unwrap();
    let manifest = ArtifactBundleManifest {
        artifact,
        segments: vec![ArtifactSegment {
            id: CapabilityId::new("weights").unwrap(),
            kind: ArtifactSegmentKind::Weights,
            offset: 0,
            length: u64::try_from(bytes.len()).unwrap(),
            digest: ArtifactDigest::from_bytes(b"poisoned"),
        }],
    };
    let reader = SegmentedModelReader::new(cache, 4, 64, 1, 64).unwrap();
    assert_eq!(
        reader.load(
            &manifest,
            &[CapabilityId::new("weights").unwrap()],
            &CancellationToken::new(),
        ),
        Err(AiError::Integrity("artifact segment digest"))
    );
    std::fs::remove_dir_all(root).unwrap();
}

fn identity(bytes: &[u8]) -> ArtifactIdentity {
    ArtifactIdentity {
        digest: ArtifactDigest::from_bytes(bytes),
        size_bytes: u64::try_from(bytes.len()).unwrap(),
        publisher: None,
        signature_required: false,
    }
}

fn segment(
    id: &str,
    kind: ArtifactSegmentKind,
    offset: usize,
    length: usize,
    bytes: &[u8],
) -> ArtifactSegment {
    ArtifactSegment {
        id: CapabilityId::new(id).unwrap(),
        kind,
        offset: u64::try_from(offset).unwrap(),
        length: u64::try_from(length).unwrap(),
        digest: ArtifactDigest::from_bytes(&bytes[offset..offset + length]),
    }
}

fn temporary_directory(name: &str) -> std::path::PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("appcore-ai-{name}-{}-{nonce}", std::process::id()))
}
