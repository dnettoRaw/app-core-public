// =============================================================================
//        #######
//     ###       ###     F: transfer.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/06 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/06 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Recovery, integrity, quotas, provider failures and bounded multi-source scheduling.
use appcore_transfer::*;
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::{
    fs,
    io::{Cursor, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        // appcore-norm: allow(global-state) reason: unique private test roots prevent concurrent fixture collisions
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        // Windows canonicalization yields a verbatim \\?\ path. The private
        // directory and TransferStore validators expect a normal absolute path.
        #[cfg(windows)]
        let temp_root = std::env::temp_dir();
        #[cfg(not(windows))]
        let temp_root = fs::canonicalize(std::env::temp_dir()).unwrap();
        let path = temp_root.join(format!(
            "transfer-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        #[cfg(unix)]
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        #[cfg(windows)]
        let _private = appcore_security::create_update_private_directory(&path).unwrap();
        #[cfg(not(any(unix, windows)))]
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn store(&self) -> TransferStore {
        TransferStore::open(&self.0, TransferLimits::default()).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn descriptor(bytes: &[u8]) -> ObjectDescriptor {
    ObjectDescriptor::from_reader(
        Cursor::new(bytes),
        bytes.len() as u64,
        8,
        TransferLimits::default(),
    )
    .unwrap()
}
fn control() -> TransferControl {
    TransferControl::new(Duration::from_secs(10)).unwrap()
}
struct Memory {
    bytes: Vec<u8>,
    mode: u8,
    calls: AtomicUsize,
    active: AtomicUsize,
    maximum: AtomicUsize,
    total: Arc<AtomicUsize>,
    max_total: Arc<AtomicUsize>,
}
impl Memory {
    fn new(bytes: &[u8], mode: u8) -> Self {
        Self {
            bytes: bytes.into(),
            mode,
            calls: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            maximum: AtomicUsize::new(0),
            total: Arc::new(AtomicUsize::new(0)),
            max_total: Arc::new(AtomicUsize::new(0)),
        }
    }
}
impl BlockSource for Memory {
    fn read_block(
        &self,
        d: &ObjectDescriptor,
        i: usize,
        c: &TransferControl,
        sink: &mut dyn Write,
    ) -> TransferResult<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.maximum.fetch_max(active, Ordering::SeqCst);
        let total = self.total.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_total.fetch_max(total, Ordering::SeqCst);
        let result = (|| {
            c.checkpoint()?;
            std::thread::sleep(Duration::from_millis(2));
            if self.mode == 1 {
                return Err(TransferError::Source);
            }
            if self.mode == 5 {
                c.cancel();
                return c.checkpoint();
            }
            let start = d.block_offset(i)? as usize;
            let mut bytes = self.bytes[start..start + d.block_len(i)?].to_vec();
            if self.mode == 2 {
                bytes[0] ^= 1;
            }
            if self.mode == 3 {
                bytes.push(0);
            }
            if self.mode == 4 {
                bytes.pop();
            }
            sink.write_all(&bytes).map_err(|_| TransferError::Source)
        })();
        self.active.fetch_sub(1, Ordering::SeqCst);
        self.total.fetch_sub(1, Ordering::SeqCst);
        result
    }
}
#[test]
fn multiple_sources_complete_exact_bytes_with_per_peer_and_global_bounds() {
    let f = Fixture::new();
    let bytes = b"opaque object spanning many independent verified blocks for two peers";
    let a = Memory::new(bytes, 0);
    let mut b = Memory::new(bytes, 0);
    b.total = a.total.clone();
    b.max_total = a.max_total.clone();
    let sources = [
        SourceBinding::approved("a", &a).unwrap(),
        SourceBinding::approved("b", &b).unwrap(),
    ];
    let mut store = f.store();
    let report = transfer(
        store.begin(descriptor(bytes)).unwrap(),
        &sources,
        TransferOptions::default(),
        &control(),
    )
    .unwrap();
    assert_eq!(fs::read(report.object.path()).unwrap(), bytes);
    assert!(report.source_bytes.iter().all(|n| *n > 0));
    assert_eq!(report.source_bytes.iter().sum::<u64>(), bytes.len() as u64);
    assert!(a.max_total.load(Ordering::SeqCst) <= 2);
    assert_eq!(a.maximum.load(Ordering::SeqCst), 1);
    assert_eq!(b.maximum.load(Ordering::SeqCst), 1);
    assert_eq!(a.total.load(Ordering::SeqCst), 0);
}
#[test]
fn dropped_session_resumes_only_verified_durable_blocks() {
    let f = Fixture::new();
    let bytes = b"resume data over several blocks";
    let d = descriptor(bytes);
    {
        let mut s = f.store();
        let mut session = s.begin(d.clone()).unwrap();
        session.accept(0, &bytes[..8]).unwrap();
        session.accept(0, &bytes[..8]).unwrap();
    }
    let provider = Memory::new(bytes, 0);
    let sources = [SourceBinding::approved("local", &provider).unwrap()];
    let mut s = f.store();
    let report = transfer(
        s.begin(d.clone()).unwrap(),
        &sources,
        TransferOptions::default(),
        &control(),
    )
    .unwrap();
    assert_eq!(report.resumed_blocks, 1);
    assert_eq!(report.attempts, d.block_count() - 1);
    assert_eq!(fs::read(report.object.path()).unwrap(), bytes);
    let cached = transfer(
        s.begin(d.clone()).unwrap(),
        &sources,
        TransferOptions::default(),
        &control(),
    )
    .unwrap();
    assert_eq!(cached.attempts, 0);
}
#[test]
fn wrong_truncated_and_oversize_sources_are_quarantined_without_bad_publication() {
    for mode in [2, 3, 4] {
        let f = Fixture::new();
        let bytes = b"recover by switching an invalid peer to a good one";
        let bad = Memory::new(bytes, mode);
        let good = Memory::new(bytes, 0);
        let sources = [
            SourceBinding::approved("bad", &bad).unwrap(),
            SourceBinding::approved("good", &good).unwrap(),
        ];
        let mut s = f.store();
        let report = transfer(
            s.begin(descriptor(bytes)).unwrap(),
            &sources,
            TransferOptions::default(),
            &control(),
        )
        .unwrap();
        assert_eq!(report.quarantined_sources, 1);
        assert_eq!(bad.calls.load(Ordering::SeqCst), 1);
        assert_eq!(report.source_bytes[0], 0);
        assert_eq!(fs::read(report.object.path()).unwrap(), bytes);
    }
}
#[test]
fn source_exhaustion_leaves_valid_progress_and_retries_are_bounded() {
    let f = Fixture::new();
    let bytes = b"bounded attempts";
    let d = descriptor(bytes);
    let source = Memory::new(bytes, 1);
    let bindings = [SourceBinding::approved("offline", &source).unwrap()];
    let mut s = f.store();
    let result = transfer(
        s.begin(d.clone()).unwrap(),
        &bindings,
        TransferOptions {
            max_attempts: 2,
            retry_delay: Duration::ZERO,
            ..TransferOptions::default()
        },
        &control(),
    );
    assert_eq!(result.unwrap_err(), TransferError::SourcesExhausted);
    assert!(source.calls.load(Ordering::SeqCst) <= d.block_count() * 2);
    assert!(!f.0.join(d.object_hash()).join("complete.bin").exists());
}
#[test]
fn cancellation_joins_workers_and_never_claims_completion() {
    let f = Fixture::new();
    let bytes = b"cancel with valid progress";
    let d = descriptor(bytes);
    let source = Memory::new(bytes, 5);
    let mut s = f.store();
    let bindings = [SourceBinding::approved("cancel", &source).unwrap()];
    let result = transfer(
        s.begin(d.clone()).unwrap(),
        &bindings,
        TransferOptions::default(),
        &control(),
    );
    assert_eq!(result.unwrap_err(), TransferError::Cancelled);
    assert_eq!(source.active.load(Ordering::SeqCst), 0);
    assert!(!f.0.join(d.object_hash()).join("complete.bin").exists());
}
#[test]
fn expired_deadline_refuses_network_work() {
    let f = Fixture::new();
    let source = Memory::new(b"deadline", 0);
    let c = TransferControl::new(Duration::from_millis(1)).unwrap();
    std::thread::sleep(Duration::from_millis(3));
    let mut s = f.store();
    let sources = [SourceBinding::approved("local", &source).unwrap()];
    assert_eq!(
        transfer(
            s.begin(descriptor(b"deadline")).unwrap(),
            &sources,
            TransferOptions::default(),
            &c
        )
        .unwrap_err(),
        TransferError::Deadline
    );
    assert_eq!(source.calls.load(Ordering::SeqCst), 0);
}
#[test]
fn corrupt_persisted_block_and_metadata_fail_closed() {
    let f = Fixture::new();
    let bytes = b"corrupt resumable bytes";
    let d = descriptor(bytes);
    {
        let mut s = f.store();
        s.begin(d.clone()).unwrap().accept(0, &bytes[..8]).unwrap();
    }
    let dir = f.0.join(d.object_hash());
    fs::write(dir.join("0000.block"), b"bad-data").unwrap();
    let mut s = f.store();
    assert!(matches!(s.begin(d.clone()), Err(TransferError::Integrity)));
    s.purge_partial(&d).unwrap();
    drop(s);
    {
        let mut s = f.store();
        drop(s.begin(d.clone()).unwrap());
    }
    let metadata = dir.join("descriptor.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&metadata).unwrap()).unwrap();
    value["format_version"] = serde_json::json!(99);
    fs::write(metadata, serde_json::to_vec(&value).unwrap()).unwrap();
    let mut s = f.store();
    assert!(matches!(s.begin(d), Err(TransferError::UnsupportedVersion)));
}
#[test]
fn orphan_staging_is_removed_but_unknown_files_are_preserved() {
    let f = Fixture::new();
    let d = descriptor(b"temporary bytes");
    let dir = f.0.join(d.object_hash());
    let block = dir.join("0000.block");
    {
        let mut s = f.store();
        s.begin(d.clone()).unwrap().accept(0, b"temporar").unwrap();
    }
    let temp = dir.join("complete.tmp");
    // Reuse a block created by TransferStore: it already has the private ACL
    // required on Windows, unlike a plain fs::write in the test harness.
    fs::rename(&block, &temp).unwrap();
    {
        let mut s = f.store();
        s.begin(d.clone()).unwrap().accept(0, b"temporar").unwrap();
    }
    assert!(!temp.exists());
    let unknown = dir.join("protected");
    fs::rename(&block, &unknown).unwrap();
    let mut s = f.store();
    assert!(matches!(s.begin(d), Err(TransferError::UnsafePath)));
    assert_eq!(fs::read(unknown).unwrap(), b"temporar");
}
#[test]
fn quota_rejects_before_block_staging_and_complete_hash_is_authoritative() {
    let f = Fixture::new();
    let bytes = b"0123456789abcdef";
    let d = descriptor(bytes);
    let mut s = f.store();
    {
        let session = s.begin(d.clone()).unwrap();
        assert!(session.finish().is_err());
    }
    let used: u64 = fs::read_dir(f.0.join(d.object_hash()))
        .unwrap()
        .map(|e| e.unwrap().metadata().unwrap().len())
        .sum();
    drop(s);
    let limits = TransferLimits {
        max_store_bytes: used + 7,
        ..TransferLimits::default()
    };
    let mut s = TransferStore::open(&f.0, limits).unwrap();
    assert_eq!(
        s.begin(d.clone()).unwrap().accept(0, &bytes[..8]),
        Err(TransferError::Limit)
    );
    drop(s);
    let mut value: serde_json::Value = serde_json::from_slice(&d.to_json().unwrap()).unwrap();
    value["object_hash"] = serde_json::json!("0".repeat(64));
    let wrong = ObjectDescriptor::from_json(
        &serde_json::to_vec(&value).unwrap(),
        TransferLimits::default(),
    )
    .unwrap();
    let mut s = f.store();
    let mut session = s.begin(wrong.clone()).unwrap();
    for i in 0..2 {
        session.accept(i, &bytes[i * 8..i * 8 + 8]).unwrap();
    }
    assert_eq!(session.finish().unwrap_err(), TransferError::Integrity);
    assert!(!f.0.join(wrong.object_hash()).join("complete.bin").exists());
}
#[test]
fn second_store_owner_is_rejected_and_complete_corruption_is_detected() {
    let f = Fixture::new();
    let mut s = f.store();
    assert!(matches!(
        TransferStore::open(&f.0, TransferLimits::default()),
        Err(TransferError::Busy)
    ));
    let bytes = b"complete bytes";
    let d = descriptor(bytes);
    let provider = Memory::new(bytes, 0);
    let sources = [SourceBinding::approved("local", &provider).unwrap()];
    let report = transfer(
        s.begin(d.clone()).unwrap(),
        &sources,
        TransferOptions::default(),
        &control(),
    )
    .unwrap();
    fs::write(report.object.path(), b"tampered").unwrap();
    assert!(matches!(
        report.object.open(),
        Err(TransferError::Integrity)
    ));
    assert!(matches!(s.begin(d), Err(TransferError::Integrity)));
}
#[test]
fn descriptors_reject_removed_versions_bad_lengths_and_unbounded_input() {
    let d = descriptor(b"descriptor");
    let mut value: serde_json::Value = serde_json::from_slice(&d.to_json().unwrap()).unwrap();
    value["format_version"] = serde_json::json!(2);
    assert_eq!(
        ObjectDescriptor::from_json(
            &serde_json::to_vec(&value).unwrap(),
            TransferLimits::default()
        )
        .unwrap_err(),
        TransferError::UnsupportedVersion
    );
    assert!(
        ObjectDescriptor::from_reader(Cursor::new(b"short"), 6, 8, TransferLimits::default())
            .is_err()
    );
    assert!(
        ObjectDescriptor::from_reader(Cursor::new(b"extra"), 4, 8, TransferLimits::default())
            .is_err()
    );
    assert!(
        ObjectDescriptor::from_json(&vec![b' '; 1024 * 1024 + 1], TransferLimits::default())
            .is_err()
    );
}
#[cfg(unix)]
#[test]
fn symlinks_and_non_private_roots_are_rejected() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let outside = Fixture::new();
    let path = f.0.join("link");
    symlink(&outside.0, &path).unwrap();
    assert!(matches!(
        TransferStore::open(path, TransferLimits::default()),
        Err(TransferError::UnsafePath)
    ));
    fs::set_permissions(&f.0, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(matches!(
        TransferStore::open(&f.0, TransferLimits::default()),
        Err(TransferError::UnsafePath)
    ));
}

#[test]
fn ttl_prunes_partial_progress_and_preserves_complete_objects() {
    let f = Fixture::new();
    let mut store = f.store();
    let partial = descriptor(b"incomplete bytes");
    store
        .begin(partial.clone())
        .unwrap()
        .accept(0, b"incomple")
        .unwrap();
    let done = descriptor(b"complete");
    let mut session = store.begin(done.clone()).unwrap();
    session.accept(0, b"complete").unwrap();
    let object = session.finish().unwrap();
    let metadata = f.0.join(partial.object_hash()).join("descriptor.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&metadata).unwrap()).unwrap();
    value["created_ms"] = 0.into();
    fs::write(&metadata, serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(store.prune_expired(Duration::from_secs(1)).unwrap(), 1);
    assert!(!f.0.join(partial.object_hash()).exists());
    assert!(object.open().is_ok());
    assert_eq!(
        store.prune_expired(Duration::ZERO).unwrap_err(),
        TransferError::InvalidContract
    );
}

#[test]
fn cancelled_assembly_keeps_durable_blocks_without_complete_file() {
    let f = Fixture::new();
    let mut store = f.store();
    let d = descriptor(b"complete");
    let mut session = store.begin(d.clone()).unwrap();
    session.accept(0, b"complete").unwrap();
    let c = control();
    c.cancel();
    assert!(matches!(
        session.finish_with_control(&c),
        Err(TransferError::Cancelled)
    ));
    assert!(!f.0.join(d.object_hash()).join("complete.bin").exists());
    assert!(store.begin(d).unwrap().missing().is_empty());
}

#[test]
fn initial_descriptor_staging_is_recovered_before_admission() {
    let f = Fixture::new();
    let d = descriptor(b"complete");
    {
        let mut store = f.store();
        drop(store.begin(d.clone()).unwrap());
    }
    let dir = f.0.join(d.object_hash());
    fs::rename(dir.join("descriptor.json"), dir.join("descriptor.tmp")).unwrap();
    let mut store = f.store();
    assert_eq!(store.begin(d).unwrap().missing(), vec![0]);
    assert!(dir.join("descriptor.json").exists());
    assert!(!dir.join("descriptor.tmp").exists());
}

#[test]
fn provider_panic_is_joined_and_quarantined_before_failover() {
    struct Panics;
    impl BlockSource for Panics {
        fn read_block(
            &self,
            _: &ObjectDescriptor,
            _: usize,
            _: &TransferControl,
            _: &mut dyn Write,
        ) -> TransferResult<()> {
            panic!("controlled provider failure");
        }
    }
    let f = Fixture::new();
    let bytes = b"bounded failover after panic";
    let d = descriptor(bytes);
    let good = Memory::new(bytes, 0);
    let sources = [
        SourceBinding::approved("panic", &Panics).unwrap(),
        SourceBinding::approved("good", &good).unwrap(),
    ];
    let mut store = f.store();
    let report = transfer(
        store.begin(d).unwrap(),
        &sources,
        TransferOptions::default(),
        &control(),
    )
    .unwrap();
    assert_eq!(report.quarantined_sources, 1);
    assert_eq!(good.active.load(Ordering::SeqCst), 0);
    assert_eq!(fs::read(report.object.path()).unwrap(), bytes);
}

#[test]
fn serde_cannot_construct_an_invalid_descriptor() {
    let d = descriptor(b"complete");
    let mut value = serde_json::to_value(d).unwrap();
    value["total_bytes"] = 0.into();
    assert!(serde_json::from_value::<ObjectDescriptor>(value).is_err());
}
