// =============================================================================
//        #######
//     ###       ###     F: main.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

mod cli;
mod command;
mod diagnostic;
mod failure;
mod introspection;
mod io;
mod output;
mod pipeline;
mod spec;

fn main() {
    let result = cli::run_env().and_then(|output| {
        let stdout = std::io::stdout();
        let mut writer = std::io::BufWriter::with_capacity(16 * 1024, stdout.lock());
        output.write_to(&mut writer)
    });
    match result {
        Ok(()) => {}
        Err(error) => {
            let stderr = std::io::stderr();
            let mut writer = std::io::BufWriter::with_capacity(4 * 1024, stderr.lock());
            let _ = error.write_to(&mut writer);
            let _ = std::io::Write::flush(&mut writer);
            std::process::exit(error.exit_code());
        }
    }
}
