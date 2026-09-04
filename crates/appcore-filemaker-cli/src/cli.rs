// =============================================================================
//        #######
//     ###       ###     F: cli.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use appcore_args::{CliParser, HelpRenderer, RawArgs};

use crate::failure::{software, CliFailure, CliResult};
use crate::output::CliOutput;

pub(crate) fn run_env() -> CliResult<CliOutput> {
    let raw = RawArgs::from_env().map_err(|error| CliFailure::usage(error.to_string(), false))?;
    run(raw)
}

fn run(raw: RawArgs) -> CliResult<CliOutput> {
    let wants_json = raw.words().iter().any(|word| word == "--json");
    let spec = crate::spec::build();
    let parsed = CliParser::new(&spec)
        .parse(&raw)
        .map_err(|error| CliFailure::usage(error.to_string(), wants_json))?;
    if parsed.has_flag("version") {
        return Ok(CliOutput::text(env!("CARGO_PKG_VERSION").to_owned()));
    }
    if parsed.has_flag("help") {
        let path = parsed
            .command_path()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        return HelpRenderer::new(&spec)
            .render(&path)
            .map(CliOutput::text)
            .map_err(|error| software(error.to_string(), wants_json));
    }
    crate::command::execute(&parsed, wants_json)
}
