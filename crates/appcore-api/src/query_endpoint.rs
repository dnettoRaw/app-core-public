// =============================================================================
//        #######
//     ###       ###     F: query_endpoint.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/31 13:38:42 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 13:45:20 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Query endpoint contracts.

pub use appcore_core::QueryName;
use appcore_core::RuntimeResult;

use crate::api::{ApiRequest, ApiResponse};

/// Contract for query endpoint handling.
pub trait QueryEndpoint: Send + Sync {
    /// Returns the unique query capability handled by this endpoint.
    fn query_name(&self) -> &QueryName;
    /// Handles one transport-neutral query request.
    fn handle_query(&self, request: ApiRequest) -> RuntimeResult<ApiResponse>;
}
