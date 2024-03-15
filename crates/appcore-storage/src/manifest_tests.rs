// =============================================================================
//        #######
//     ###       ###     F: manifest_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/06/04 11:57:41 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;

fn create_test_file(dir: &Path, rel_path: &str, content: &[u8]) {
    let path = dir.join(rel_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

#[test]
fn test_manifest_validation() {
    let temp_dir = std::env::temp_dir().join(format!("manifest-test-{}", std::process::id()));
    fs::create_dir_all(&temp_dir).unwrap();

    let file1 = "db.json";
    let file2 = "logs/sync.log";
    create_test_file(&temp_dir, file1, b"hello world");
    create_test_file(&temp_dir, file2, b"sync checkpoint 123");

    let manifest = StorageManifest::generate(
        "app-1",
        "node-1",
        "0.6.0-beta.1",
        1000,
        &temp_dir,
        &[file1, file2],
    )
    .unwrap();

    assert_eq!(manifest.files.len(), 2);
    assert!(manifest.verify(&temp_dir).is_ok());

    // Arquivo modificado deve falhar a validação.
    create_test_file(&temp_dir, file1, b"hello modified");
    let verify_result_mod = manifest.verify(&temp_dir);
    assert!(verify_result_mod.is_err());

    // Restaura arquivo 1.
    create_test_file(&temp_dir, file1, b"hello world");

    // Arquivo ausente deve falhar a validação.
    fs::remove_file(temp_dir.join(file2)).unwrap();
    let verify_result_miss = manifest.verify(&temp_dir);
    assert!(verify_result_miss.is_err());

    // Tamanho incorreto deve falhar a validação.
    create_test_file(&temp_dir, file2, b"sync checkpoint 1234");
    let verify_result_size = manifest.verify(&temp_dir);
    assert!(verify_result_size.is_err());

    // Versão incompatível de schema deve falhar.
    let mut bad_manifest = manifest.clone();
    bad_manifest.schema_version = "2".to_string();
    let verify_result_schema = bad_manifest.verify(&temp_dir);
    assert!(matches!(
        verify_result_schema,
        Err(StorageError::MigrationFailed(_))
    ));

    let _ = fs::remove_dir_all(temp_dir);
}
