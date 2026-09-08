// =============================================================================
//        #######
//     ###       ###     F: hello_world.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/04 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/04 00:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

//! Start here: one dependency, no manifest files and no hidden services.
//!
//! The builder uses V4 by default. Keep it in a local variable when several
//! events share the same application context.

use appcore_sdk::prelude::*;

fn main() -> AppResult<()> {
    appcore_sdk::run("hello-world", |app| {
        // `App::logger` carries the application identifier into every event.
        let log = app.logger();

        log.info("Hello, world!");

        Ok(())
    })
}
