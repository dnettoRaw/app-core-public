// =============================================================================
//        #######
//     ###       ###     F: explicit_manifests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/04 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/04 00:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

//! Keep the same `App` shape while replacing zero-config contracts explicitly.
//!
//! This example begins with validated local defaults only to make copies that
//! an application can inspect or replace before it starts its callback.

use appcore_sdk::prelude::*;

fn main() -> AppResult<()> {
    // The defaults are regular V1 contracts, not a second SDK format.
    let defaults = App::new("explicit-manifests")?;

    let application = defaults.effective_application_manifest().clone();
    let deployment = defaults.effective_deployment_manifest().clone();

    // Explicit inputs replace the defaults after their normal validation.
    App::new("explicit-manifests")?
        .application_manifest(application)?
        .deployment_manifest(deployment)?
        .run(|app| {
            let log = app.logger().component("manifest");

            log.info("Explicit V1 manifests are now in effect");

            Ok(())
        })
}
