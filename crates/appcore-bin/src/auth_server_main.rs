// =============================================================================
//        #######
//     ###       ###     F: auth_server_main.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/06 22:13:35 by dnettoRaw
//    ##   ## ##   ##    U: 2026/06/06 22:13:35 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Optional companion auth-server binary entrypoint.

use appcore_bin::auth_server::run_auth_server_env;

fn main() {
    if let Err(error) = run_auth_server_env() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
