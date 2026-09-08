// =============================================================================
//        #######
//     ###       ###     F: file_logging.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: unknown by dnettoRaw
//    ##   ## ##   ##    U: working-tree by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Persist shareable events in terminal and bounded JSONL files.
//!
//! JSONL is for Safe or Diagnostic output only. Sensitive diagnostics use the
//! encrypted DNT sink shown in the separate example.

use appcore_log::{
    FileArchiveConfig, FileSinkConfig, LogConfigError, LogError, LogOutputMode, LogPolicy,
    LoggerConfig, Verbosity, LOG_SIZE_8_MIB,
};
use std::path::PathBuf;

fn main() -> Result<(), appcore_log::LogConfigError> {
    // Keep generated output predictable and inside Cargo's ignored target tree.
    let output_directory = PathBuf::from("target/appcore-log-example");
    std::fs::create_dir_all(&output_directory).map_err(|_| LogConfigError::Sink(LogError::Io))?;

    let active_file = output_directory.join("application.jsonl");

    // Two rotations stay beside the active file. Older rotations move into
    // archive/YYYY/MM and the complete archive never exceeds 120 files.
    let archive_directory = output_directory.join("archive");
    let mut policy = LogPolicy::new(Verbosity::V4);
    policy.set_component("sync", Verbosity::V8);

    let logger = LoggerConfig {
        policy,
        output: LogOutputMode::TerminalAndFile,
        file: Some(FileSinkConfig {
            path: active_file,
            max_bytes: LOG_SIZE_8_MIB,
            sync_each_write: false,
            retention: 2,
            archive: Some(FileArchiveConfig {
                directory: archive_directory,
                max_files: 120,
            }),
        }),
        ..LoggerConfig::default()
    }
    .build()?;

    let application = logger.dispatcher().event(0, "application");
    let sync = logger.dispatcher().event(1, "sync.transport");

    application.info("application ready; inspect target/appcore-log-example/application.jsonl");

    // The parent component policy makes this V7 diagnostic visible.
    sync.verbosity(7).debug("replication batch sent");

    sync.warn("peer response was delayed");

    let stats = logger.dispatcher().stats();
    assert_eq!(stats.sink_failures, 0);

    Ok(())
}
