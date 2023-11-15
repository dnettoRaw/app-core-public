// =============================================================================
//        #######
//     ###       ###     F: idempotency_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::{
    FileIdempotencyStore, IdempotencyRecord, IdempotencyStatus, IdempotencyStore,
    InMemoryIdempotencyStore,
};
use std::time::{SystemTime, UNIX_EPOCH};

fn assert_send_sync<T: Send + Sync>() {}

fn temp_file(name: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("appcore-idemp-{name}-{nanos}.txt"))
}

#[test]
fn in_memory_store_registers_and_finds_keys() {
    assert_send_sync::<InMemoryIdempotencyStore>();
    let mut store = InMemoryIdempotencyStore::new();
    assert!(store.get("k1").unwrap().is_none());
    let record = IdempotencyRecord {
        key: "k1".to_string(),
        request_hash: "cmd-1".to_string(),
        status: IdempotencyStatus::Pending,
        created_at_ms: 1000,
    };
    assert!(store.insert(record).is_ok());
    assert!(store.get("k1").unwrap().is_some());
    assert_eq!(store.len(), 1);
}

#[test]
fn store_debug_output_does_not_expose_record_contents() {
    let record = IdempotencyRecord {
        key: "sensitive-key".to_string(),
        request_hash: "sensitive-request-hash".to_string(),
        status: IdempotencyStatus::Resolved {
            response_status: 200,
            response_body: "sensitive-response-body".to_string(),
        },
        created_at_ms: 1000,
    };
    let mut memory = InMemoryIdempotencyStore::new();
    memory.insert(record.clone()).unwrap();

    let file = temp_file("debug-redaction");
    let mut persisted = FileIdempotencyStore::new(&file).unwrap();
    persisted.insert(record).unwrap();

    for output in [format!("{memory:?}"), format!("{persisted:?}")] {
        assert!(output.contains("entry_count: 1"));
        assert!(!output.contains("sensitive-key"));
        assert!(!output.contains("sensitive-request-hash"));
        assert!(!output.contains("sensitive-response-body"));
    }

    let _ = std::fs::remove_file(file);
}

#[test]
fn file_store_saves_and_reloads() {
    assert_send_sync::<FileIdempotencyStore>();
    let file = temp_file("reload");
    let mut store = match FileIdempotencyStore::new(&file) {
        Ok(store) => store,
        Err(_) => return,
    };
    let record1 = IdempotencyRecord {
        key: "k1".to_string(),
        request_hash: "cmd-1".to_string(),
        status: IdempotencyStatus::Resolved {
            response_status: 200,
            response_body: "{}".to_string(),
        },
        created_at_ms: 1000,
    };
    let record2 = IdempotencyRecord {
        key: "k2".to_string(),
        request_hash: "cmd-2".to_string(),
        status: IdempotencyStatus::Resolved {
            response_status: 201,
            response_body: "{}".to_string(),
        },
        created_at_ms: 1000,
    };
    assert!(store.insert(record1).is_ok());
    assert!(store.insert(record2).is_ok());
    drop(store);

    let reloaded = match FileIdempotencyStore::new(&file) {
        Ok(store) => store,
        Err(_) => return,
    };
    assert_eq!(reloaded.len(), 2);
    assert!(reloaded.get("k1").unwrap().is_some());

    let _ = std::fs::remove_file(file);
}

#[test]
fn file_store_rejects_invalid_key() {
    let file = temp_file("invalid");
    let mut store = match FileIdempotencyStore::new(&file) {
        Ok(store) => store,
        Err(_) => return,
    };
    let record = IdempotencyRecord {
        key: "../bad".to_string(),
        request_hash: "cmd-1".to_string(),
        status: IdempotencyStatus::Pending,
        created_at_ms: 1000,
    };
    let result = store.insert(record);
    assert_eq!(
        result,
        Err(crate::RuntimeError::InvalidIdempotencyKey {
            reason: "invalid_char"
        })
    );
    assert!(store.is_empty());
    assert_eq!(
        std::fs::read_to_string(&file).ok().as_deref(),
        Some("# appcore-idempotency-v1\n")
    );
    let _ = std::fs::remove_file(file);
}

#[test]
fn unversioned_line_format_is_rejected_with_upgrade_wall() {
    let file = temp_file("unversioned");
    assert!(std::fs::write(&file, "k1\tcmd-1\n").is_ok());
    let error = FileIdempotencyStore::new_with_ttl(&file, Some(1)).unwrap_err();
    assert!(format!("{error:?}").contains("NO MORE SUPPORTED PLEASE UPDATE"));
    let _ = std::fs::remove_file(file);
}

#[test]
fn expired_entry_is_ignored_with_ttl() {
    let file = temp_file("expired");
    let record = r#"{"key":"k1","request_hash":"cmd-1","status":{"Resolved":{"response_status":200,"response_body":""}},"created_at_ms":1}"#;
    assert!(std::fs::write(
        &file,
        format!("{}\n{record}\n", super::IDEMPOTENCY_FORMAT_V1)
    )
    .is_ok());
    let store = match FileIdempotencyStore::new_with_ttl(&file, Some(10)) {
        Ok(store) => store,
        Err(_) => return,
    };
    assert!(store.get("k1").unwrap().is_none());
    let _ = std::fs::remove_file(file);
}

#[test]
fn compact_removes_expired_and_keeps_valid() {
    let file = temp_file("compact");
    let first = r#"{"key":"k1","request_hash":"cmd-1","status":{"Resolved":{"response_status":200,"response_body":""}},"created_at_ms":1}"#;
    let second = r#"{"key":"k2","request_hash":"cmd-2","status":{"Resolved":{"response_status":200,"response_body":""}},"created_at_ms":9999999999999}"#;
    let data = format!("{}\n{first}\n{second}\n", super::IDEMPOTENCY_FORMAT_V1);
    assert!(std::fs::write(&file, data).is_ok());
    let mut store = match FileIdempotencyStore::new_with_ttl(&file, Some(10)) {
        Ok(store) => store,
        Err(_) => return,
    };
    let removed = store.compact(1000);
    assert!(removed.is_ok());
    assert_eq!(removed.unwrap_or(0), 1);
    assert!(store.get("k1").unwrap().is_none());
    assert!(store.get("k2").unwrap().is_some());
    let _ = std::fs::remove_file(file);
}

#[test]
fn file_store_ignores_pending_at_startup() {
    let file = temp_file("ignore-pending");
    let pending_record =
        r#"{"key":"k1","request_hash":"hash1","status":"Pending","created_at_ms":1000}"#;
    let resolved_record = r#"{"key":"k2","request_hash":"hash2","status":{"Resolved":{"response_status":200,"response_body":"{}"}},"created_at_ms":1000}"#;
    assert!(std::fs::write(
        &file,
        format!(
            "{}\n{pending_record}\n{resolved_record}\n",
            super::IDEMPOTENCY_FORMAT_V1
        )
    )
    .is_ok());

    let store = FileIdempotencyStore::new(&file).unwrap();
    assert_eq!(store.len(), 1);
    assert!(store.get("k1").unwrap().is_none());
    assert!(store.get("k2").unwrap().is_some());

    let _ = std::fs::remove_file(file);
}

#[test]
fn file_store_rejects_unversioned_tsv() {
    let file = temp_file("unversioned-tsv-test");
    let data = "k1\tcmd-1\t1000\nk2\tcmd-2\ncorrupted_line_without_tabs\n";
    assert!(std::fs::write(&file, data).is_ok());

    assert!(matches!(
        FileIdempotencyStore::new(&file),
        Err(crate::RuntimeError::IdempotencyStoreIo {
            operation: "validate_store",
            message,
        })
            if message == "NO MORE SUPPORTED PLEASE UPDATE"
    ));

    let _ = std::fs::remove_file(file);
}

#[test]
fn file_store_recovers_only_an_incomplete_final_record() {
    let file = temp_file("partial-tail");
    let complete = r#"{"key":"k1","request_hash":"hash","status":{"Resolved":{"response_status":200,"response_body":"{}"}},"created_at_ms":1}"#;
    std::fs::write(
        &file,
        format!(
            "{}\n{complete}\n{{\"key\":\"partial",
            super::IDEMPOTENCY_FORMAT_V1
        ),
    )
    .unwrap();

    let store = FileIdempotencyStore::new(&file).unwrap();
    assert!(store.get("k1").unwrap().is_some());
    let recovered = std::fs::read_to_string(&file).unwrap();
    assert!(!recovered.contains("partial"));
    assert!(recovered.starts_with(super::IDEMPOTENCY_FORMAT_V1));
    std::fs::remove_file(file).unwrap();
}

#[test]
fn file_store_rejects_future_format() {
    let file = temp_file("future-format");
    std::fs::write(&file, "# appcore-idempotency-v2\n").unwrap();

    assert!(FileIdempotencyStore::new(&file).is_err());
    std::fs::remove_file(file).unwrap();
}

#[test]
fn file_store_concurrent_stress_test() {
    use parking_lot::Mutex;
    use std::sync::Arc;

    let file = temp_file("stress");
    let store = Arc::new(Mutex::new(FileIdempotencyStore::new(&file).unwrap()));
    let thread_count = 10;
    let inserts_per_thread = 50;
    let mut handles = Vec::new();

    for t in 0..thread_count {
        let store_clone = Arc::clone(&store);
        let handle = std::thread::spawn(move || {
            for i in 0..inserts_per_thread {
                let key = format!("thread-{t}-key-{i}");
                let record = IdempotencyRecord {
                    key: key.clone(),
                    request_hash: format!("hash-{t}-{i}"),
                    status: IdempotencyStatus::Resolved {
                        response_status: 200,
                        response_body: "{}".to_string(),
                    },
                    created_at_ms: 1000 + i as u64,
                };
                let mut guard = store_clone.lock();
                assert!(guard.insert(record).is_ok());
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    // Extract inner store from Arc to drop it and wait for worker thread to finish flushing
    let inner_store = Arc::try_unwrap(store)
        .map_err(|_| "Arc has sole owner")
        .unwrap();
    drop(inner_store);

    // Now reload from disk and check if all records are there
    let reloaded = FileIdempotencyStore::new(&file).unwrap();
    assert_eq!(reloaded.len(), thread_count * inserts_per_thread);

    let _ = std::fs::remove_file(file);
}
