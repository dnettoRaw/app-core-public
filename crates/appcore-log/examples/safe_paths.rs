// =============================================================================
//        #######
//     ###       ###     F: safe_paths.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: unknown by dnettoRaw
//    ##   ## ##   ##    U: working-tree by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Typed paths are alias-sanitized before ordinary sinks receive them.
//!
//! This example builds `LogEvent` directly because it demonstrates a typed path
//! field. Free-form message text must not carry filesystem paths or secrets.

use appcore_log::{LogEvent, LogPolicy, Severity, Verbosity};

fn main() {
    let mut policy = LogPolicy::default();

    // The deployment provides trusted aliases instead of exposing host paths.
    policy.paths.app_root = Some("/application".to_string());

    let event = LogEvent::new(0, Severity::Info, Verbosity::V4, "storage", "opened")
        .path("file", "/application/data/log.jsonl");

    // Ordinary sinks receive `<APP_ROOT>/data/log.jsonl`, never the raw path.
    let _safe = policy.sanitize(&event);
}
