// =============================================================================
//        #######
//     ###       ###     F: observation_file_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;
use crate::{InMemoryObservationSink, ObservationKind, ObservationSeverity};

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "appcore-observations-{name}-{}.jsonl",
        std::process::id()
    ))
}

fn event(index: u64) -> ObservationEvent {
    ObservationEvent::new(
        ObservationKind::Lifecycle,
        ObservationSeverity::Info,
        format!("runtime.test.{index}"),
        index,
    )
}

#[test]
fn file_sink_writes_versioned_redacted_jsonl() {
    let path = temp_path("write");
    let _ = fs::remove_file(&path);
    let sink = FileObservationSink::new(FileObservationSinkConfig::new(&path)).unwrap();
    sink.emit(event(1).with_attribute("access_token", "secret"));
    sink.flush().unwrap();

    let text = fs::read_to_string(&path).unwrap();
    assert!(text.starts_with(OBSERVATION_FILE_FORMAT_V1));
    assert!(text.contains("runtime.test.1"));
    assert!(text.contains("[REDACTED]"));
    assert!(!text.contains("\"secret\""));
    assert_eq!(sink.stats().written, 1);
    drop(sink);
    fs::remove_file(path).unwrap();
}

#[test]
fn in_memory_hub_forwards_to_file_drain() {
    let path = temp_path("hub");
    let _ = fs::remove_file(&path);
    let drain = Arc::new(FileObservationSink::new(FileObservationSinkConfig::new(&path)).unwrap());
    let hub = InMemoryObservationSink::new(4);
    hub.add_drain(drain.clone());
    hub.emit(event(2));
    drain.flush().unwrap();

    assert_eq!(hub.drain_count(), 1);
    assert!(fs::read_to_string(&path)
        .unwrap()
        .contains("runtime.test.2"));
    drop(hub);
    drop(drain);
    fs::remove_file(path).unwrap();
}

#[test]
fn file_sink_rotates_and_retains_bounded_files() {
    let path = temp_path("rotation");
    let _ = fs::remove_file(&path);
    for index in 1..=4 {
        let _ = fs::remove_file(rotated_path(&path, index));
    }
    let mut config = FileObservationSinkConfig::new(&path);
    config.max_file_bytes = 64 * 1024;
    config.retained_files = 2;
    config.sync_every_records = 1;
    let sink = FileObservationSink::new(config).unwrap();
    for index in 0..900 {
        sink.emit(event(index).with_attribute("padding", "x".repeat(512)));
    }
    sink.flush().unwrap();
    drop(sink);

    assert!(path.exists());
    assert!(rotated_path(&path, 1).exists());
    assert!(rotated_path(&path, 2).exists());
    assert!(!rotated_path(&path, 3).exists());
    fs::remove_file(&path).unwrap();
    fs::remove_file(rotated_path(&path, 1)).unwrap();
    fs::remove_file(rotated_path(&path, 2)).unwrap();
}

#[cfg(unix)]
#[test]
fn file_sink_rejects_symlink_destination() {
    use std::os::unix::fs::symlink;

    let path = temp_path("symlink");
    let target = temp_path("symlink-target");
    let _ = fs::remove_file(&path);
    let _ = fs::remove_file(&target);
    fs::write(&target, b"target").unwrap();
    symlink(&target, &path).unwrap();

    assert!(FileObservationSink::new(FileObservationSinkConfig::new(&path)).is_err());
    fs::remove_file(path).unwrap();
    fs::remove_file(target).unwrap();
}
