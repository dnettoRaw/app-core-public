// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/19 12:52:57 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/19 13:34:54 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

#![doc = include_str!("../README.en.md")]
#![forbid(unsafe_code)]

mod completion;
mod error;
mod help;
mod parser;
mod raw;
mod shell;
mod spec;
mod spec_validation;
mod suggestion;
mod value_parser;

#[cfg(test)]
mod parser_tests;
#[cfg(test)]
mod spec_tests;

pub use completion::{CompletionCandidate, CompletionEngine, CompletionKind, CompletionRequest};
pub use error::{CliError, CliErrorKind};
pub use help::HelpRenderer;
pub use parser::{CliParser, ParsedCli, ParsedOption};
pub use raw::{ArgLimits, RawArgs};
pub use shell::{render_dynamic_completion_script, Shell, ShellScriptError};
pub use spec::{ArgumentSpec, CliSpec, CommandSpec, OptionSpec, SpecError, ValueMode, ValueType};
