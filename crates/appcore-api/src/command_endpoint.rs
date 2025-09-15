// =============================================================================
//        #######
//     ###       ###     F: command_endpoint.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/31 13:38:42 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 13:45:20 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Command endpoint contract.

use appcore_core::RuntimeResult;

use crate::api::{ApiRequest, ApiResponse};

/// Contract for command endpoint handling.
pub trait CommandEndpoint: Send + Sync {
    /// Handles one transport-neutral command request.
    fn handle_command(&self, request: ApiRequest) -> RuntimeResult<ApiResponse>;
}
