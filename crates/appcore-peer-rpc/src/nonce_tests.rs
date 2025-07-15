// =============================================================================
//        #######
//     ###       ###     F: nonce_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;

fn test_path(name: &str) -> PathBuf {
    std::env::temp_dir()
        .join(format!(
            "appcore-peer-nonce-{name}-{}-{}",
            std::process::id(),
            now_nanos()
        ))
        .join("security/nonces.json")
}

#[test]
fn file_nonce_store_rejects_replay_across_instances_and_restart() {
    let path = test_path("restart");
    let first = FilePeerNonceStore::open(&path).unwrap();
    let second = FilePeerNonceStore::open(&path).unwrap();

    assert_eq!(first.check_and_record("nonce-a", 100, 10), Ok(()));
    assert_eq!(
        second.check_and_record("nonce-a", 100, 20),
        Err(PeerRpcError::NonceReplay)
    );
    drop(first);
    drop(second);

    let restarted = FilePeerNonceStore::open(&path).unwrap();
    assert_eq!(
        restarted.check_and_record("nonce-a", 100, 30),
        Err(PeerRpcError::NonceReplay)
    );
    assert_eq!(restarted.check_and_record("nonce-a", 200, 100), Ok(()));
    fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).unwrap();
}

#[test]
fn file_nonce_store_serializes_concurrent_recording() {
    let path = test_path("concurrent");
    let first = FilePeerNonceStore::open(&path).unwrap();
    let second = FilePeerNonceStore::open(&path).unwrap();
    let first_thread = std::thread::spawn(move || first.check_and_record("nonce-a", 100, 10));
    let second_thread = std::thread::spawn(move || second.check_and_record("nonce-a", 100, 10));
    let results = [first_thread.join().unwrap(), second_thread.join().unwrap()];

    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| **result == Err(PeerRpcError::NonceReplay))
            .count(),
        1
    );
    fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).unwrap();
}

#[test]
fn file_nonce_store_fails_closed_on_corrupt_state() {
    let path = test_path("corrupt");
    let store = FilePeerNonceStore::open(&path).unwrap();
    store.check_and_record("nonce-a", 100, 10).unwrap();
    fs::write(&path, b"{partial").unwrap();
    set_private_path(&path);

    assert!(matches!(
        store.check_and_record("nonce-b", 100, 10),
        Err(PeerRpcError::InvalidEnvelope(reason)) if reason == "nonce_store_state_corrupt"
    ));
    fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).unwrap();
}

#[cfg(unix)]
fn set_private_path(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

#[cfg(not(unix))]
fn set_private_path(_path: &Path) {}
