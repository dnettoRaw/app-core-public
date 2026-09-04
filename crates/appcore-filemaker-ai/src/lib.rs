// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Optional bounded tool bridge from `appcore-ai` to deterministic `FileMaker` sessions.

mod error;
mod mutation;
mod policy;
mod query;
mod session;
mod tools;

pub use error::{BridgeError, BridgeResult};
pub use policy::AiBridgePolicy;
pub use session::{FileMakerAiSession, ToolExecution};
pub use tools::{recommended_tool_loop, tool_definitions};
