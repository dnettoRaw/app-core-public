// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/29 20:47:35 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! API contracts for command/query routing without transport implementation.

#![deny(missing_docs)]

#[cfg(all(feature = "insecure-testing", not(debug_assertions)))]
compile_error!("insecure-testing cannot be enabled in release builds");

#[deny(missing_docs)]
pub mod api;
#[deny(missing_docs)]
pub mod command_contract;
#[deny(missing_docs)]
pub mod command_endpoint;
pub mod http;
#[deny(missing_docs)]
pub mod query_contract;
#[deny(missing_docs)]
pub mod query_endpoint;
#[deny(missing_docs)]
pub mod router;

pub use api::{ApiMethod, ApiRequest, ApiResponse};
pub use command_contract::{
    CommandRequest, CommandRequestValidationError, CommandResponse, CommandResponseEvent,
};
pub use command_endpoint::CommandEndpoint;
pub use http::{
    CommandCapabilityPolicy, CommandCapabilityPolicyError, CommandTokenVerifier, HttpApiConfig,
    HttpCommandAuth, HttpReloadPhase, HttpReloadPolicy, HttpReloadSnapshot,
    HttpRoutingGenerationSnapshot, HttpRoutingGenerationsSnapshot, PreparedRuntimeHttpGeneration,
    ReloadableRuntimeHttpHost, RequestPayloadRef, RequestValidationDetails,
    RequestValidationDetailsRef, RuntimeHttpHost, RuntimeHttpReloadError, RuntimeHttpStateParts,
    RuntimeStaticInfo, SyncLogView, SyncLogViewError,
};
pub use query_contract::{QueryRequest, QueryRequestValidationError, QueryResponse};
pub use query_endpoint::{QueryEndpoint, QueryName};
pub use router::ApiRouter;
