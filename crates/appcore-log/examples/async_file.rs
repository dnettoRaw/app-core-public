// =============================================================================
//        #######
//     ###       ###     F: async_file.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/07 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/07 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Explicit bounded asynchronous file delivery with a visible output path.

use appcore_log::{
    AsyncSink, AsyncSinkConfig, FileSink, FileSinkConfig, LogDispatcher, LogError, LogPolicy,
    LOG_SIZE_8_MIB,
};
use std::sync::Arc;

fn main() -> Result<(), LogError> {
    let directory = std::path::PathBuf::from("target/appcore-log-example");
    std::fs::create_dir_all(&directory).map_err(|_| LogError::Io)?;

    let path = directory.join("async.jsonl");
    let file = Arc::new(FileSink::new(FileSinkConfig {
        path: path.clone(),
        max_bytes: LOG_SIZE_8_MIB,
        sync_each_write: true,
        retention: 2,
        archive: None,
    })?);

    // This queue retains at most 256 events and 1 MiB, including active I/O.
    let asynchronous = Arc::new(AsyncSink::new(
        AsyncSinkConfig {
            max_events: 256,
            max_bytes: 1024 * 1024,
        },
        file,
    )?);
    let dispatcher = LogDispatcher::new(LogPolicy::default(), vec![asynchronous.clone()]);
    let log = dispatcher.event(0, "application");

    log.info("application started");
    log.warn("storage response is slow");

    // The lifecycle owner drains durable writes before exiting.
    asynchronous.shutdown()?;
    let path = std::fs::canonicalize(path).map_err(|_| LogError::Io)?;
    println!("log written to {}", path.display());
    Ok(())
}
