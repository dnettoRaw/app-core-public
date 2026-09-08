// =============================================================================
//        #######
//     ###       ###     F: application_contract.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/04 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/04 00:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

//! Declare business contracts without constructing Runtime infrastructure.
//!
//! An application registers its own behavior while `AppCore` owns the generic
//! Runtime lifecycle and infrastructure.

use appcore_sdk::application::{CommandName, CommandRegistry, NodeId, RuntimeResult};
use appcore_sdk::{App, AppResult, Application};

struct ReportingApp;

impl Application for ReportingApp {
    fn register_commands(&self, registry: &mut CommandRegistry) -> RuntimeResult<()> {
        // Command names describe business behavior without starting a host.
        registry.register(CommandName::new("report.generate")?)
    }
}

fn main() -> AppResult<()> {
    let app = App::new("reporting")?;
    let prepared = app.prepare(&ReportingApp, NodeId::new("reporting-local")?)?;

    assert_eq!(prepared.runtime().commands().len(), 1);

    Ok(())
}
