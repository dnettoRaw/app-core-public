// =============================================================================
//        #######
//     ###       ###     F: store_io_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/03 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/03 00:00:00 by dnettoRaw
//      ###########      S: 1.0.3-rc
// =============================================================================
// appcore-norm: test

use super::*;
use serde::Deserialize;
use std::io::Cursor;

#[derive(Debug, Deserialize, PartialEq, Serialize)]
struct Metadata<'a> {
    format_version: u16,
    #[serde(borrow)]
    value: &'a str,
}

#[derive(Debug, Deserialize)]
struct OwnedMetadata {
    value: String,
}

#[test]
fn json_writer_preserves_pretty_encoding_without_result_buffer() {
    let root = test_root("exact");
    let _ = fs::remove_dir_all(&root);
    let path = root.join("metadata.json");
    let value = Metadata {
        format_version: 1,
        value: "candidate",
    };

    atomic_write_json(&path, &value).unwrap();

    assert_eq!(
        fs::read(&path).unwrap(),
        serde_json::to_vec_pretty(&value).unwrap()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn oversized_json_fails_before_creating_the_output() {
    let root = test_root("oversized");
    let _ = fs::remove_dir_all(&root);
    let path = root.join("metadata.json");
    let oversized = "x".repeat(MAX_UPDATE_METADATA_BYTES);
    let value = Metadata {
        format_version: 1,
        value: &oversized,
    };

    assert!(atomic_write_json(&path, &value).is_err());
    assert!(!path.exists());
    assert!(!root.exists());
}

#[test]
fn json_reader_accepts_exact_limit_and_detects_growth() {
    let mut encoded = serde_json::to_vec(&Metadata {
        format_version: 1,
        value: "candidate",
    })
    .unwrap();
    encoded.resize(MAX_UPDATE_METADATA_BYTES, b' ');
    let decoded: OwnedMetadata = decode_json_bounded(
        Cursor::new(&encoded),
        u64::try_from(encoded.len()).unwrap(),
        MAX_UPDATE_METADATA_BYTES,
    )
    .unwrap();
    assert_eq!(decoded.value, "candidate");

    encoded.push(b' ');
    let error = decode_json_bounded::<OwnedMetadata>(
        Cursor::new(encoded),
        MAX_UPDATE_METADATA_BYTES as u64,
        MAX_UPDATE_METADATA_BYTES,
    )
    .unwrap_err();
    assert!(matches!(error, JsonReadError::Io(_)));
}

#[test]
fn json_reader_distinguishes_missing_io_and_decode_failures() {
    let root = test_root("reader-errors");
    let _ = fs::remove_dir_all(&root);
    let path = root.join("metadata.json");
    assert!(read_json_bounded::<serde_json::Value>(&path, 16)
        .unwrap()
        .is_none());

    let decode = decode_json_bounded::<serde_json::Value>(Cursor::new(b"{"), 1, 16).unwrap_err();
    assert!(matches!(decode, JsonReadError::Decode(_)));
}

fn test_root(suffix: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "appcore-update-json-{suffix}-{}",
        std::process::id()
    ))
}
