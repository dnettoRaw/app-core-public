// =============================================================================
//        #######
//     ###       ###     F: api.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/31 13:38:42 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 13:45:20 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Raw API request/response contracts.

/// Supported API method kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiMethod {
    /// Mutating or important command.
    Command,
    /// Side-effect-free query.
    Query,
    /// Runtime health probe.
    Health,
}

/// Transport-neutral API request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiRequest {
    /// Logical request method.
    pub method: ApiMethod,
    /// Transport-neutral endpoint or capability path.
    pub path: String,
    /// Opaque request bytes owned by the caller.
    pub payload: Vec<u8>,
}

/// Transport-neutral API response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiResponse {
    /// HTTP-compatible status code.
    pub status_code: u16,
    /// Opaque response bytes owned by the endpoint.
    pub payload: Vec<u8>,
}

#[cfg(test)]
#[path = "api_tests.rs"]
mod tests;
