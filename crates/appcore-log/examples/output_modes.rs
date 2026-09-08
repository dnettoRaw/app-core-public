// =============================================================================
//        #######
//     ###       ###     F: output_modes.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: working-tree by dnettoRaw
//    ##   ## ##   ##    U: working-tree by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Select exactly one bounded logging behavior at application startup.

use appcore_log::{FileSinkConfig, LogOutputMode, LoggerConfig, LOG_SIZE_2_MIB};

fn main() -> Result<(), appcore_log::LogConfigError> {
    // Replace this with Disabled, Terminal, File, TerminalAndFile or CrashOnly.
    let output = LogOutputMode::CrashOnly;

    let logger = LoggerConfig {
        output,
        file: Some(FileSinkConfig {
            path: std::env::temp_dir().join("my-application-crash.jsonl"),
            max_bytes: LOG_SIZE_2_MIB,
            sync_each_write: true,
            retention: 1,
            archive: None,
        }),
        crash_events: 128,
        crash_bytes: 512 * 1024,
        ..LoggerConfig::default()
    }
    .build()?;

    let log = logger.dispatcher().event(0, "application");

    log.info("kept only in the bounded crash ring");

    // Call this from the application's panic/crash boundary. In every other
    // mode it is a no-op and returns zero.
    let _written_events = logger.dump_crash()?;

    Ok(())
}
