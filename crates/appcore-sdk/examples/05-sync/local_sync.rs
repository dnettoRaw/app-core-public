// =============================================================================
//        #######
//     ###       ###     F: local_sync.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/04 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/04 00:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

//! Use the bounded in-memory outbox before selecting a deployment transport.
//!
//! The normal V4 event is suitable for operators; the temporary V7 event is
//! available only when the component policy has been raised accordingly.

use appcore_sdk::sync::{InMemorySyncOutbox, SyncOutbox};

fn main() -> Result<(), String> {
    // This local outbox does not imply a remote transport or cluster mode.
    let outbox = InMemorySyncOutbox::new();

    let _stats = outbox.stats().map_err(|error| format!("{error:?}"))?;

    appcore_sdk::run("local-sync", |app| {
        let log = app.logger().component("sync");

        log.info("Sync capability selected explicitly");

        // The local default remains V4 after this single diagnostic event.
        log.verbosity(7)
            .debug("Deep synchronization diagnostics are opt-in");

        Ok(())
    })
    .map_err(|error| error.to_string())?;
    Ok(())
}
