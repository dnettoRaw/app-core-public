// =============================================================================
//        #######
//     ###       ###     F: main.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/29 20:47:35 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 13:18:47 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Runtime bootstrap binary composition root.

use appcore_bin::bootstrap::BootstrapError;
use appcore_bin::cli::parse_cli_env;
use appcore_bin::commands::run_cli;

fn main() {
    let result = parse_cli_env()
        .map_err(|error| BootstrapError::Cli(error.to_string()))
        .and_then(run_cli);
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(i32::from(error.exit_code()));
    }
}
