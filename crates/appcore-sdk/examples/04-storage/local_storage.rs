// =============================================================================
//        #######
//     ###       ###     F: local_storage.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/04 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/04 00:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

//! Add the local storage capability without changing application startup.
//!
//! Storage remains an opt-in capability: constructing a provider does not make
//! the SDK start a listener or select a deployment host.

use appcore_sdk::prelude::*;
use appcore_sdk::storage::FileStorageProvider;

fn main() -> AppResult<()> {
    appcore_sdk::run("local-storage", |app| {
        // Application-owned paths stay explicit at the storage boundary.
        let _storage = FileStorageProvider::new("./data", "./backups");

        let log = app.logger().component("storage");

        log.info("Local storage provider configured");

        Ok(())
    })
}
