// =============================================================================
//        #######
//     ###       ###     F: error.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/19 12:52:57 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/19 13:34:54 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CliErrorKind {
    InvalidInput,
    InvalidSpecification,
    UnknownOption,
    UnexpectedArgument,
    MissingCommand,
    MissingOption,
    MissingValue,
    InvalidValue,
    DuplicateOption,
    OptionConflict,
    MissingRequirement,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CliError {
    kind: CliErrorKind,
    message: String,
}

impl CliError {
    pub fn new(kind: CliErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn kind(&self) -> CliErrorKind {
        self.kind
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CliError {}
