// =============================================================================
//        #######
//     ###       ###     F: tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: unknown by dnettoRaw
//    ##   ## ##   ##    U: working-tree by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

// appcore-norm: test

use super::*;
use appcore_contracts::ApplicationId;
use appcore_dnt::{inspect_header, KeyId, SecretKey, StaticDntKeyProvider};
use std::path::PathBuf;
use std::sync::Arc;

fn temporary_log_path(label: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    std::env::temp_dir().join(format!(
        "appcore-log-{label}-{}-{}.jsonl",
        std::process::id(),
        nonce
    ))
}

fn event(verbosity: Verbosity) -> LogEvent {
    LogEvent::new(10, Severity::Info, verbosity, "sync", "token=abc hello")
        .path("path", "/work/data/a.db")
        .secret("api_key", "abc")
}

#[test]
fn file_sink_reuses_state_without_losing_rotation_bounds() {
    let path = temporary_log_path("persistent-rotation");
    let sample = LogEvent::new(20, Severity::Info, Verbosity::V4, "file", "bounded");
    let encoded_bytes = crate::json_line::encode(&sample, 4096).unwrap().len() as u64;
    let sink = FileSink::new(FileSinkConfig {
        path: path.clone(),
        max_bytes: encoded_bytes * 2,
        sync_each_write: false,
        retention: 1,
        archive: None,
    })
    .unwrap();

    sink.emit(&sample).unwrap();
    sink.emit(&sample).unwrap();
    sink.emit(&sample).unwrap();

    let active = std::fs::read_to_string(&path).unwrap();
    let rotated_path = path.with_extension("jsonl.1");
    let rotated = std::fs::read_to_string(&rotated_path).unwrap();
    assert_eq!(active.lines().count(), 1);
    assert_eq!(rotated.lines().count(), 2);
    assert!(active.len() as u64 <= encoded_bytes * 2);
    assert!(rotated.len() as u64 <= encoded_bytes * 2);

    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(rotated_path).unwrap();
}

#[test]
fn file_sink_reopens_after_external_file_replacement() {
    let path = temporary_log_path("external-replacement");
    let moved = path.with_extension("moved.jsonl");
    let sink = FileSink::new(FileSinkConfig {
        path: path.clone(),
        max_bytes: 4096,
        sync_each_write: false,
        retention: 1,
        archive: None,
    })
    .unwrap();

    sink.emit(&LogEvent::new(
        21,
        Severity::Info,
        Verbosity::V4,
        "file",
        "before replacement",
    ))
    .unwrap();
    std::fs::rename(&path, &moved).unwrap();
    sink.emit(&LogEvent::new(
        22,
        Severity::Info,
        Verbosity::V4,
        "file",
        "after replacement",
    ))
    .unwrap();

    let old_content = std::fs::read_to_string(&moved).unwrap();
    let new_content = std::fs::read_to_string(&path).unwrap();
    assert!(old_content.contains("before replacement"));
    assert!(!old_content.contains("after replacement"));
    assert!(new_content.contains("after replacement"));

    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(moved).unwrap();
}

#[test]
fn verbosity_filters_independently_from_severity() {
    let ring = Arc::new(RingBufferSink::new(4, 4096).unwrap());
    let dispatcher = LogDispatcher::new(LogPolicy::new(Verbosity::V4), vec![ring.clone()]);
    dispatcher.emit(event(Verbosity::V5));
    dispatcher.emit(LogEvent::new(
        11,
        Severity::Critical,
        Verbosity::V1,
        "sync",
        "down",
    ));
    assert_eq!(ring.snapshot().len(), 1);
    assert_eq!(dispatcher.stats().filtered, 1);
}

#[test]
fn safe_policy_redacts_secrets_and_aliases_paths() {
    let mut policy = LogPolicy::default();
    policy.paths.app_root = Some("/work".to_string());
    let sanitized = policy.sanitize(&event(Verbosity::V4));
    assert_eq!(sanitized.message, "<REDACTED> hello");
    assert_eq!(sanitized.fields[0].value, "<APP_ROOT>/data/a.db");
    assert_eq!(sanitized.fields[1].value, "<REDACTED>");
}

#[test]
fn safe_policy_redacts_credentials_embedded_in_text() {
    let policy = LogPolicy::default();
    let sanitized = policy.sanitize(&LogEvent::new(
        10,
        Severity::Warn,
        Verbosity::V3,
        "network",
        "url=https://user:password@example.invalid token=abc cookie:session",
    ));
    assert!(!sanitized.message.contains("password"));
    assert!(!sanitized.message.contains("token=abc"));
    assert!(!sanitized.message.contains("session"));
}

#[test]
fn safe_policy_aliases_home_and_hides_unknown_local_paths() {
    let mut policy = LogPolicy::default();
    policy.paths.home = Some("/Users/dan".to_string());
    let home = policy.sanitize(
        &LogEvent::new(10, Severity::Info, Verbosity::V4, "storage", "opened")
            .path("path", "/Users/dan/.local/share/app/data.db"),
    );
    let unknown = policy.sanitize(
        &LogEvent::new(11, Severity::Info, Verbosity::V4, "storage", "opened")
            .path("path", "/srv/private/app/data.db"),
    );
    assert_eq!(home.fields[0].value, "<HOME>/.local/share/app/data.db");
    assert_eq!(unknown.fields[0].value, "<LOCAL_PATH>");
}

#[test]
fn ring_buffer_enforces_its_capacity() {
    let ring = RingBufferSink::new(1, 4096).unwrap();
    ring.emit(&event(Verbosity::V4)).unwrap();
    ring.emit(&LogEvent::new(
        11,
        Severity::Info,
        Verbosity::V4,
        "sync",
        "next",
    ))
    .unwrap();
    assert_eq!(ring.snapshot().len(), 1);
    assert_eq!(ring.snapshot()[0].message, "next");
}

#[test]
fn sensitive_sink_writes_only_authenticated_dnt() {
    let path = std::env::temp_dir().join(format!("appcore-log-{}.dnt", std::process::id()));
    let key_id = KeyId::new("log-key").unwrap();
    let provider = StaticDntKeyProvider::new().with_key(key_id.clone(), SecretKey::new([7; 32]));
    let sink = SensitiveDntSink::new(
        SensitiveDntSinkConfig {
            path: path.clone(),
            application_id: ApplicationId::new("log-app").unwrap(),
            key_id,
            max_bytes: 4096,
            max_events: 4,
            retention: 1,
        },
        provider,
    )
    .unwrap();
    sink.emit(&event(Verbosity::V9)).unwrap();
    sink.emit(&LogEvent::new(
        11,
        Severity::Error,
        Verbosity::V2,
        "security",
        "rotated snapshot",
    ))
    .unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert!(inspect_header(&bytes).is_ok());
    assert!(!bytes.windows(3).any(|value| value == b"abc"));
    assert!(path.with_extension("dnt.1").is_file());
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_file(path.with_extension("dnt.1")).unwrap();
}

#[test]
fn sensitive_policy_never_delivers_to_an_ordinary_sink() {
    let ring = Arc::new(RingBufferSink::new(4, 4096).unwrap());
    let mut policy = LogPolicy::default();
    policy.sensitivity = Sensitivity::Sensitive;
    let dispatcher = LogDispatcher::new(policy, vec![ring.clone()]);
    dispatcher.emit(LogEvent::new(
        12,
        Severity::Error,
        Verbosity::V2,
        "security",
        "credential diagnostic",
    ));
    assert!(ring.snapshot().is_empty());
    assert_eq!(dispatcher.stats().sink_failures, 1);
}

#[test]
fn component_override_does_not_change_the_global_threshold() {
    let ring = Arc::new(RingBufferSink::new(4, 4096).unwrap());
    let mut policy = LogPolicy::new(Verbosity::V4);
    policy.set_component("sync", Verbosity::V9);
    let dispatcher = LogDispatcher::new(policy, vec![ring.clone()]);
    dispatcher.emit(LogEvent::new(
        13,
        Severity::Debug,
        Verbosity::V8,
        "sync",
        "accepted",
    ));
    dispatcher.emit(LogEvent::new(
        14,
        Severity::Debug,
        Verbosity::V8,
        "storage",
        "filtered",
    ));
    assert_eq!(ring.snapshot().len(), 1);
    assert_eq!(dispatcher.stats().filtered, 1);
}

#[test]
fn parent_component_override_applies_to_descendants() {
    let ring = Arc::new(RingBufferSink::new(4, 4096).unwrap());
    let mut policy = LogPolicy::new(Verbosity::V4);
    policy.set_component("sync", Verbosity::V8);
    let dispatcher = LogDispatcher::new(policy, vec![ring.clone()]);
    dispatcher.emit(LogEvent::new(
        15,
        Severity::Debug,
        Verbosity::V7,
        "sync.transport",
        "accepted",
    ));
    assert_eq!(ring.snapshot().len(), 1);
}

#[test]
fn oversized_event_is_rejected_before_sink_delivery() {
    let ring = Arc::new(RingBufferSink::new(4, 8192).unwrap());
    let dispatcher = LogDispatcher::new(LogPolicy::default(), vec![ring.clone()]);
    dispatcher.emit(LogEvent::new(
        16,
        Severity::Info,
        Verbosity::V4,
        "application",
        "x".repeat(MAX_LOG_TEXT_BYTES + 1),
    ));
    assert!(ring.snapshot().is_empty());
    assert_eq!(dispatcher.stats().invalid, 1);
}

#[test]
fn injected_clock_produces_a_deterministic_timestamp() {
    let ring = Arc::new(RingBufferSink::new(4, 4096).unwrap());
    let dispatcher = LogDispatcher::new(LogPolicy::default(), vec![ring.clone()]);
    dispatcher
        .event_now(&FixedLogClock::new(42), "application")
        .info("clocked");
    assert_eq!(ring.snapshot()[0].timestamp_ms, 42);
}

#[derive(Debug)]
struct FailingSink;

impl LogSink for FailingSink {
    fn emit(&self, _: &LogEvent) -> Result<(), LogError> {
        Err(LogError::Io)
    }

    fn name(&self) -> &'static str {
        "test_failure"
    }
}

#[test]
fn sink_failures_are_accounted_per_sink_without_recursion() {
    let dispatcher = LogDispatcher::new(LogPolicy::default(), vec![Arc::new(FailingSink)]);
    dispatcher.emit(LogEvent::new(
        17,
        Severity::Warn,
        Verbosity::V3,
        "application",
        "controlled failure",
    ));
    assert_eq!(dispatcher.stats().sink_failures, 1);
    assert_eq!(dispatcher.sink_stats()[0].name, "test_failure");
    assert_eq!(dispatcher.sink_stats()[0].failures, 1);
}

#[test]
fn concurrent_emission_preserves_ring_bounds() {
    let ring = Arc::new(RingBufferSink::new(16, 16 * 1024).unwrap());
    let dispatcher = Arc::new(LogDispatcher::new(LogPolicy::default(), vec![ring.clone()]));
    let workers = (0..4)
        .map(|worker| {
            let dispatcher = Arc::clone(&dispatcher);
            std::thread::spawn(move || {
                for sequence in 0..16 {
                    dispatcher.emit(LogEvent::new(
                        worker * 100 + sequence,
                        Severity::Info,
                        Verbosity::V4,
                        "concurrent",
                        "bounded",
                    ));
                }
            })
        })
        .collect::<Vec<_>>();
    for worker in workers {
        worker.join().unwrap();
    }
    assert!(ring.snapshot().len() <= 16);
}

#[test]
fn fluent_builder_reuses_defaults_with_a_per_event_override() {
    let ring = Arc::new(RingBufferSink::new(4, 4096).unwrap());
    let mut policy = LogPolicy::new(Verbosity::V4);
    policy.set_component("sync", Verbosity::V8);
    let dispatcher = LogDispatcher::new(policy, vec![ring.clone()]);
    let log = dispatcher.event(15, "application").component("sync");
    log.info("default event");
    log.verbosity(7).info("deep event");
    assert_eq!(ring.snapshot().len(), 2);
    assert_eq!(ring.snapshot()[0].component, "sync");
    assert_eq!(ring.snapshot()[0].verbosity, Verbosity::V4);
    assert_eq!(ring.snapshot()[1].verbosity, Verbosity::V7);
}

#[test]
fn checked_verbosity_rejects_invalid_configuration() {
    let dispatcher = LogDispatcher::new(LogPolicy::default(), Vec::new());
    assert!(matches!(
        dispatcher.event(18, "application").try_verbosity(0),
        Err(VerbosityError)
    ));
    assert!(dispatcher.event(18, "application").try_verbosity(9).is_ok());
}

struct PanicMessage;

impl From<PanicMessage> for String {
    fn from(_: PanicMessage) -> Self {
        panic!("disabled logging formatted a message")
    }
}

#[test]
fn disabled_dispatcher_does_not_convert_the_message() {
    let logger = LoggerConfig {
        output: LogOutputMode::Disabled,
        ..LoggerConfig::default()
    }
    .build()
    .unwrap();
    logger.dispatcher().event(19, "disabled").info(PanicMessage);
}
