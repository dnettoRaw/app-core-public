// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 13:21:42 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Official Vercel plus Neon control-plane adapter.
//!
//! Runtime nodes call the Vercel HTTPS control-plane API. Neon remains an
//! implementation detail of that API and its credentials never enter a Runtime
//! deployment manifest.

#![deny(missing_docs)]

mod factory;

pub use factory::{
    SharedControlPlaneProvider, VercelNeonControlPlaneFactory, AUTH_TOKEN_SECRET,
    VERCEL_NEON_PROVIDER_ID,
};

#[cfg(test)]
mod tests;
