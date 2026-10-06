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
use crate::spec::ValueType;
use crate::suggestion;

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
