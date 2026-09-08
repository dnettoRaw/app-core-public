// =============================================================================
//        #######
//     ###       ###     F: ai_request.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/04 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/04 00:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

//! Construct and validate a bounded AI request without selecting a backend.
//!
//! Choosing `LocalOnly` keeps the example deterministic: no model, network or
//! remote provider is inferred by merely validating the request contract.

use appcore_sdk::ai::{AiExecutionMode, AiLimits, AiPrivacyMode, AiRequest, AiTask};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let limits = AiLimits::default();

    let mut request = AiRequest::text(AiTask::ClassifyText, "status", limits)?;

    request.options.execution = AiExecutionMode::Local;
    request.options.privacy = AiPrivacyMode::LocalOnly;

    request.validate(limits)?;

    appcore_sdk::run("ai-request", |app| {
        let log = app.logger().component("ai");

        log.info("AI request validated; backend selection remains explicit");

        Ok(())
    })?;
    Ok(())
}
