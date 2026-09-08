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
use crate::{
    InMemoryObservationSink, ObservationKind, ObservationSeverity, MAX_OBSERVATION_ATTRIBUTES,
    MAX_OBSERVATION_VALUE_BYTES,
};
use std::time::{Duration, Instant};

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

#[test]
fn event_serialization_streams_chunks_with_an_exact_size_pass() {
    let config = FileObservationSinkConfig::new(temp_path("streaming"));
    let event = padded_event("x");
    let expected = measure_event_line(config.max_file_bytes, &event).unwrap();
    let mut writer = TrackingWriter::default();

    write_event_line(&mut writer, &event).unwrap();

    assert_eq!(writer.total, expected);
    assert!(writer.maximum_write < writer.total);
}

#[test]
fn file_sink_rejects_a_record_that_cannot_fit_an_empty_file() {
    let path = temp_path("oversized-record");
    let _ = fs::remove_file(&path);
    let mut config = FileObservationSinkConfig::new(&path);
    config.max_file_bytes = 64 * 1024;
    config.sync_every_records = 1;
    let sink = FileObservationSink::new(config).unwrap();

    sink.emit(padded_event("\\"));
    sink.flush().unwrap();

    assert_eq!(sink.stats().written, 0);
    assert_eq!(sink.stats().errors, 1);
    assert_eq!(
        fs::read(&path).unwrap(),
        format!("{OBSERVATION_FILE_FORMAT_V1}\n").as_bytes()
    );
    assert!(!rotated_path(&path, 1).exists());
    drop(sink);
    fs::remove_file(path).unwrap();
}

#[test]
fn queue_budget_rejects_aggregate_overflow_and_releases_on_drop() {
    let budget = Arc::new(QueueBudget::new(10));
    let reservation = budget.reserve(6).unwrap();

    assert!(budget.reserve(5).is_none());
    assert_eq!(
        budget.pressure(),
        FileObservationSinkPressure {
            queued_bytes: 6,
            max_queue_bytes: 10,
            peak_queued_bytes: 6,
            byte_rejections: 1,
        }
    );

    drop(reservation);
    assert_eq!(budget.pressure().queued_bytes, 0);
}

#[test]
fn file_sink_reports_released_queue_pressure_after_flush() {
    let path = temp_path("pressure");
    let _ = fs::remove_file(&path);
    let sink = FileObservationSink::new(FileObservationSinkConfig::new(&path)).unwrap();

    sink.emit(event(3));
    sink.flush().unwrap();

    let pressure = sink.pressure();
    assert_eq!(pressure.queued_bytes, 0);
    assert_eq!(pressure.max_queue_bytes, MAX_FILE_OBSERVATION_QUEUE_BYTES);
    assert!(pressure.peak_queued_bytes > 0);
    assert_eq!(pressure.byte_rejections, 0);
    drop(sink);
    fs::remove_file(path).unwrap();
}

#[test]
fn flush_deadline_bounds_queue_admission_and_acknowledgement() {
    let (sender, _blocked_receiver) = mpsc::sync_channel(1);
    let (occupied_acknowledge, _occupied_receiver) = mpsc::channel();
    assert!(sender
        .try_send(DrainCommand::Flush(occupied_acknowledge))
        .is_ok());
    let (pending_acknowledge, _pending_receiver) = mpsc::channel();
    let admission = crate::observation_flush::enqueue(
        &sender,
        DrainCommand::Flush(pending_acknowledge),
        Instant::now() + Duration::from_millis(5),
    )
    .unwrap_err();
    assert_eq!(admission.kind(), std::io::ErrorKind::TimedOut);

    let (available, available_receiver) = mpsc::sync_channel(1);
    let (late_acknowledge, _late_receiver) = mpsc::channel();
    let late = crate::observation_flush::enqueue(
        &available,
        DrainCommand::Flush(late_acknowledge),
        Instant::now(),
    )
    .unwrap_err();
    assert_eq!(late.kind(), std::io::ErrorKind::TimedOut);
    assert!(available_receiver.try_recv().is_err());

    let (_unanswered, receiver) = mpsc::channel();
    let acknowledgement =
        crate::observation_flush::wait(receiver, Instant::now() + Duration::from_millis(5))
            .unwrap_err();
    assert_eq!(acknowledgement.kind(), std::io::ErrorKind::TimedOut);
}

#[test]
fn flush_rejects_zero_timeout_without_queueing_work() {
    let path = temp_path("zero-flush-timeout");
    let _ = fs::remove_file(&path);
    let sink = FileObservationSink::new(FileObservationSinkConfig::new(&path)).unwrap();

    let error = sink.flush_timeout(Duration::ZERO).unwrap_err();

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    let error = sink.flush_timeout(Duration::MAX).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    drop(sink);
    fs::remove_file(path).unwrap();
}

#[test]
fn file_sink_rejects_excessive_item_capacity() {
    let path = temp_path("capacity");
    let mut config = FileObservationSinkConfig::new(path);
    config.queue_capacity = MAX_FILE_OBSERVATION_QUEUE_ITEMS + 1;

    assert!(FileObservationSink::new(config).is_err());
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

fn padded_event(character: &str) -> ObservationEvent {
    let mut event = event(1);
    for index in 0..MAX_OBSERVATION_ATTRIBUTES {
        event = event.with_attribute(
            format!("padding-{index}"),
            character.repeat(MAX_OBSERVATION_VALUE_BYTES),
        );
    }
    event
}

#[derive(Default)]
struct TrackingWriter {
    total: u64,
    maximum_write: u64,
}

impl Write for TrackingWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let bytes = buffer.len() as u64;
        self.total += bytes;
        self.maximum_write = self.maximum_write.max(bytes);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
