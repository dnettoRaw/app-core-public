// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/05 18:38:23 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Library API for booting and running the AppCore runtime host.

// `application` is the stable application-facing facade. Other public modules
// exist only where the runtime CLI or host integration requires them.
#[deny(missing_docs)]
pub mod application;
#[deny(missing_docs)]
pub mod application_context;
#[deny(missing_docs)]
pub mod application_host;
mod application_plugin;
mod application_supervisor;
mod application_tasks;
pub mod auth_server;
pub mod auth_server_grant;
mod auth_server_install;
pub(crate) mod auth_server_network;
mod capability_policy;
pub use auth_server_network::AuthServerHosting;
pub mod bootstrap;
pub mod build_info;
pub mod cli;
pub mod commands;
pub mod constants;
pub(crate) mod control_plane_service;
mod gateway_service;
pub mod local_lifecycle;
mod managed_health;
mod manifest_bootstrap;
mod manifests;
pub mod paths;
pub(crate) mod peer_rpc_service;
mod providers;
pub mod runtime_config;
mod runtime_services;
mod scheduler_service;
pub mod security_cli;
pub mod server;
pub mod supervisor;
pub mod sync_cli;
mod update_service;

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
