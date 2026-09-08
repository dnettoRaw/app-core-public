// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/04 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/04 00:00:00 by dnettoRaw
//      ###########      S: 1.0.1
// =============================================================================

//! Retirement notice for the former AppCore host and CLI package.
//!
//! Use [`appcore-sdk`](https://docs.rs/appcore-sdk) for new and migrated
//! applications. This crate deliberately contains no executable, host,
//! compatibility layer, or Runtime dependency.

/// Migration documentation for applications that used `appcore-bin`.
#[deprecated(note = "appcore-bin is retired; depend on appcore-sdk")]
pub const MIGRATION_GUIDE: &str = "https://wiki.appcore.dnettoraw.com/crates/appcore-sdk";
