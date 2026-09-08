// =============================================================================
//        #######
//     ###       ###     F: configured_logging.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: working-tree by dnettoRaw
//    ##   ## ##   ##    U: working-tree by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

//! Configure application logging without creating a host or background worker.

use appcore_sdk::logging::{FileSinkConfig, LogOutputMode, LoggerConfig, LOG_SIZE_2_MIB};
use appcore_sdk::prelude::*;

fn main() -> AppResult<()> {
    let logger = LoggerConfig {
        output: LogOutputMode::CrashOnly,
        file: Some(FileSinkConfig {
            path: std::env::temp_dir().join("example-application-crash.jsonl"),
            max_bytes: LOG_SIZE_2_MIB,
            sync_each_write: true,
            retention: 1,
            archive: None,
        }),
        crash_events: 128,
        crash_bytes: 512 * 1024,
        ..LoggerConfig::default()
    };

    App::new("configured-logging")?.logging(logger)?.run(|app| {
        let log = app.logger().component("startup");

        log.info("application initialized");

        // A real application calls this from its crash boundary.
        let _written = app.dump_crash_log()?;

        Ok(())
    })
}
