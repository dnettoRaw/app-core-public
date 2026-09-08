// =============================================================================
//        #######
//     ###       ###     F: basic.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: unknown by dnettoRaw
//    ##   ## ##   ##    U: working-tree by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Emit one safe structured operational event.
//!
//! Applications normally receive the same builder from
//! `appcore_sdk::App::logger`.

use appcore_log::{LogOutputMode, LoggerConfig};

fn main() -> Result<(), appcore_log::LogConfigError> {
    // Safe V4 terminal logging is explicit and creates no global state.
    let logger = LoggerConfig {
        output: LogOutputMode::Terminal,
        ..LoggerConfig::default()
    }
    .build()?;

    // The builder defaults to V4 and can be kept for related events.
    let log = logger.dispatcher().event(0, "application");

    log.info("application started");
    log.warn("connection is slower than expected");
    log.error("document could not be saved");

    Ok(())
}
