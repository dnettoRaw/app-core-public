// =============================================================================
//        #######
//     ###       ###     F: value_parser.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/20 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/20 00:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use crate::error::{CliError, CliErrorKind};
use crate::spec::{OptionSpec, ValueMode, ValueType};
use crate::suggestion;

pub(crate) fn option_value(
    words: &[String],
    index: usize,
    inline: Option<String>,
    option: &OptionSpec,
    short: bool,
) -> Result<(Option<String>, usize), CliError> {
    match option.value_mode() {
        ValueMode::Forbidden if inline.is_some() && !short => Err(CliError::new(
            CliErrorKind::InvalidValue,
            format!("option `--{}` does not accept a value", option.long()),
        )),
        ValueMode::Forbidden => Ok((None, index + 1)),
        ValueMode::Optional => optional_value(words, index, inline, option),
        ValueMode::Required => {
            if let Some(value) = inline {
                Ok((Some(value), index + 1))
            } else if let Some(value) = words.get(index + 1) {
                Ok((Some(value.clone()), index + 2))
            } else {
                Err(CliError::new(
                    CliErrorKind::MissingValue,
                    format!("option `--{}` requires a value", option.long()),
                ))
            }
        }
    }
}

pub(crate) fn validate_value(
    value: &str,
    value_type: ValueType,
    possible: &[String],
    owner: &str,
) -> Result<(), CliError> {
    let typed = match value_type {
        ValueType::String => true,
        ValueType::Bool => matches!(value, "true" | "false"),
        ValueType::I64 => value.parse::<i64>().is_ok(),
        ValueType::U64 => value.parse::<u64>().is_ok(),
    };
    if !typed {
        return Err(CliError::new(
            CliErrorKind::InvalidValue,
            format!("invalid value `{value}` for `{owner}`; expected {value_type}"),
        ));
    }
    if !possible.is_empty() && !possible.iter().any(|candidate| candidate == value) {
        let message = suggestion::append(
            format!(
                "invalid value `{value}` for `{owner}`; expected one of: {}",
                possible.join(", ")
            ),
            suggestion::closest(value, possible.iter().map(String::as_str)),
            "",
        );
        return Err(CliError::new(CliErrorKind::InvalidValue, message));
    }
    Ok(())
}

fn optional_value(
    words: &[String],
    index: usize,
    inline: Option<String>,
    option: &OptionSpec,
) -> Result<(Option<String>, usize), CliError> {
    if inline.is_some() || !option.accepts_detached_optional_value() {
        return Ok((inline, index + 1));
    }
    let Some(candidate) = words.get(index + 1) else {
        return Ok((None, index + 1));
    };
    if validate_value(
        candidate,
        option.value_type_kind(),
        option.possible_values(),
        &format!("--{}", option.long()),
    )
    .is_ok()
    {
        Ok((Some(candidate.clone()), index + 2))
    } else {
        Ok((None, index + 1))
    }
}
