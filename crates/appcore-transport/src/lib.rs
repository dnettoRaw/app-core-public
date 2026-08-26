// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Bounded blocking HTTP/1.1 and TLS primitives for infrastructure adapters.
//!
//! This crate owns transport mechanics only. Control plane, peer RPC, sync and
//! application APIs retain their own authentication, retry and status policy.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod client;
mod connection;
mod pool;
mod response;
mod target;
mod types;
mod wire;

pub use client::{send, HttpClient};
pub use response::{decode_gzip_limited, encode_gzip_if_smaller, parse_response};
pub use target::{HttpScheme, HttpTarget};
pub use types::{
    CancellationToken, HttpClientConfig, HttpExchangeConfig, HttpHeader, HttpPoolConfig,
    HttpRequest, HttpResponse, HttpTimeouts, TransportError, TransportResult,
};

#[cfg(test)]
mod tests;
