// =============================================================================
//        #######
//     ###       ###     F: data.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded data contracts and behavior for this crate.

use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::{ErrorCode, Expression, ExpressionBudget, FileMakerError, Result};

/// Exact monetary value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrencyValue {
    /// ISO-4217-style uppercase code.
    pub code: String,
    /// Exact decimal amount.
    #[serde(with = "rust_decimal::serde::str")]
    pub amount: Decimal,
}

/// Typed data accepted by bindings and datasets.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum DataValue {
    /// UTF-8 string.
    String(String),
    /// Signed integer.
    Integer(i64),
    /// Exact decimal.
    Decimal(#[serde(with = "rust_decimal::serde::str")] Decimal),
    /// Boolean.
    Boolean(bool),
    /// ISO `YYYY-MM-DD` date retained losslessly.
    Date(String),
    /// RFC-3339-like date-time retained losslessly.
    DateTime(String),
    /// Signed duration in milliseconds.
    Duration(i64),
    /// Exact monetary value.
    Currency(CurrencyValue),
    /// Ordered array.
    Array(Vec<Self>),
    /// Deterministically ordered object.
    Object(BTreeMap<String, Self>),
    /// Explicit null.
    Null,
}

impl DataValue {
    /// Resolves a dot-separated object path without side effects.
    #[must_use]
    pub fn get_path(&self, path: &str) -> Option<&Self> {
        if path.is_empty() {
            return Some(self);
        }
        let mut value = self;
        for part in path.split('.') {
            value = match value {
                Self::Object(object) => object.get(part)?,
                Self::Array(array) => array.get(part.parse::<usize>().ok()?)?,
                _ => return None,
            };
        }
        Some(value)
    }

    /// Returns a bounded display representation used by text bindings.
    #[must_use]
    pub fn display(&self) -> String {
        match self {
            Self::String(value) | Self::Date(value) | Self::DateTime(value) => value.clone(),
            Self::Integer(value) | Self::Duration(value) => value.to_string(),
            Self::Decimal(value) => value.normalize().to_string(),
            Self::Boolean(value) => value.to_string(),
            Self::Currency(value) => format!("{} {}", value.amount.normalize(), value.code),
            Self::Array(_) => "[array]".to_owned(),
            Self::Object(_) => "[object]".to_owned(),
            Self::Null => String::new(),
        }
    }

    /// Returns truthiness for conditional rules.
    #[must_use]
    pub fn is_truthy(&self) -> bool {
        match self {
            Self::Boolean(value) => *value,
            Self::Null => false,
            Self::String(value) => !value.is_empty(),
            Self::Integer(value) | Self::Duration(value) => *value != 0,
            Self::Decimal(value) => !value.is_zero(),
            Self::Currency(value) => !value.amount.is_zero(),
            Self::Array(value) => !value.is_empty(),
            Self::Object(value) => !value.is_empty(),
            Self::Date(_) | Self::DateTime(_) => true,
        }
    }

    /// Validates bounded structural invariants.
    pub fn validate(
        &self,
        max_depth: usize,
        max_items: usize,
        max_text_bytes: usize,
    ) -> Result<()> {
        let mut stack = vec![(self, 0_usize)];
        let mut count = 0_usize;
        while let Some((value, depth)) = stack.pop() {
            if depth > max_depth {
                return Err(data_error("data nesting exceeds configured depth"));
            }
            count = count.saturating_add(1);
            if count > max_items {
                return Err(data_error("data item count exceeds configured limit"));
            }
            match value {
                Self::String(value) => {
                    if value.len() > max_text_bytes {
                        return Err(data_error("data string exceeds configured byte limit"));
                    }
                }
                Self::Date(value) => {
                    if value.len() > max_text_bytes || !valid_date(value) {
                        return Err(data_error("date must use a valid YYYY-MM-DD value"));
                    }
                }
                Self::DateTime(value) => {
                    if value.len() > max_text_bytes || !valid_date_time(value) {
                        return Err(data_error("date-time must use a valid RFC-3339-like value"));
                    }
                }
                Self::Currency(value) if !valid_currency_code(&value.code) => {
                    return Err(data_error(
                        "currency code must be three uppercase ASCII letters",
                    ));
                }
                Self::Array(values) => {
                    stack.extend(values.iter().rev().map(|item| (item, depth + 1)));
                }
                Self::Object(values) => {
                    if values.keys().any(|key| key.is_empty() || key.len() > 128) {
                        return Err(data_error("object key is empty or too long"));
                    }
                    stack.extend(values.values().rev().map(|item| (item, depth + 1)));
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Declared data type.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataType {
    /// String.
    String,
    /// Integer.
    Integer,
    /// Decimal.
    Decimal,
    /// Boolean.
    Boolean,
    /// Date.
    Date,
    /// Date-time.
    DateTime,
    /// Duration.
    Duration,
    /// Currency.
    Currency,
    /// Array.
    Array,
    /// Object.
    Object,
    /// Explicit null.
    Null,
}

/// One schema field.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DataField {
    /// Declared type.
    pub data_type: DataType,
    /// Whether null is accepted.
    pub nullable: bool,
    /// Optional deterministic computed expression.
    pub computed: Option<String>,
}

/// Deterministically ordered schema.
pub type DataSchema = BTreeMap<String, DataField>;

/// Resolves computed schema fields through a bounded deterministic dependency graph.
///
/// Caller values for computed fields are replaced by their declared expression,
/// making the schema the single source of truth for derived values.
pub fn resolve_computed_fields(
    schema: &DataSchema,
    data: &DataValue,
    max_expression_steps: usize,
) -> Result<DataValue> {
    let DataValue::Object(input) = data else {
        return Err(data_error("computed schema root requires an object"));
    };
    let mut values = input.clone();
    let mut resolved = std::collections::BTreeSet::new();
    let mut visiting = std::collections::BTreeSet::new();
    for name in schema.keys() {
        resolve_computed_field(
            name,
            schema,
            &mut values,
            &mut resolved,
            &mut visiting,
            max_expression_steps,
        )?;
    }
    let result = DataValue::Object(values);
    validate_schema(schema, &result)?;
    Ok(result)
}

fn resolve_computed_field(
    name: &str,
    schema: &DataSchema,
    values: &mut BTreeMap<String, DataValue>,
    resolved: &mut std::collections::BTreeSet<String>,
    visiting: &mut std::collections::BTreeSet<String>,
    max_expression_steps: usize,
) -> Result<()> {
    if resolved.contains(name) {
        return Ok(());
    }
    let Some(field) = schema.get(name) else {
        return Ok(());
    };
    let Some(source) = &field.computed else {
        resolved.insert(name.to_owned());
        return Ok(());
    };
    if !visiting.insert(name.to_owned()) {
        return Err(FileMakerError::new(
            ErrorCode::DataCycle,
            format!("computed data cycle includes `{name}`"),
        ));
    }
    let expression = Expression::parse(source.clone())?;
    for dependency in expression.dependencies() {
        if schema
            .get(&dependency)
            .is_some_and(|field| field.computed.is_some())
        {
            resolve_computed_field(
                &dependency,
                schema,
                values,
                resolved,
                visiting,
                max_expression_steps,
            )?;
        }
    }
    let root = DataValue::Object(values.clone());
    let value = expression.evaluate(&root, &mut ExpressionBudget::new(max_expression_steps)?)?;
    values.insert(name.to_owned(), value);
    visiting.remove(name);
    resolved.insert(name.to_owned());
    Ok(())
}

/// Validates one object against a schema.
pub fn validate_schema(schema: &DataSchema, data: &DataValue) -> Result<()> {
    let DataValue::Object(object) = data else {
        return Err(data_error("schema root requires an object"));
    };
    for (name, field) in schema {
        let Some(value) = object.get(name) else {
            if field.nullable || field.computed.is_some() {
                continue;
            }
            return Err(data_error(format!(
                "required data field `{name}` is missing"
            )));
        };
        if matches!(value, DataValue::Null) && field.nullable {
            continue;
        }
        if !matches_type(value, field.data_type) {
            return Err(data_error(format!(
                "data field `{name}` has the wrong type"
            )));
        }
    }
    Ok(())
}

fn matches_type(value: &DataValue, expected: DataType) -> bool {
    matches!(
        (value, expected),
        (DataValue::String(_), DataType::String)
            | (DataValue::Integer(_), DataType::Integer)
            | (DataValue::Decimal(_), DataType::Decimal)
            | (DataValue::Boolean(_), DataType::Boolean)
            | (DataValue::Date(_), DataType::Date)
            | (DataValue::DateTime(_), DataType::DateTime)
            | (DataValue::Duration(_), DataType::Duration)
            | (DataValue::Currency(_), DataType::Currency)
            | (DataValue::Array(_), DataType::Array)
            | (DataValue::Object(_), DataType::Object)
            | (DataValue::Null, DataType::Null)
    )
}

fn valid_currency_code(code: &str) -> bool {
    code.len() == 3 && code.bytes().all(|byte| byte.is_ascii_uppercase())
}

fn valid_date(value: &str) -> bool {
    if !value.is_ascii() || value.len() != 10 || &value[4..5] != "-" || &value[7..8] != "-" {
        return false;
    }
    let Some(year) = decimal(&value[0..4]) else {
        return false;
    };
    let Some(month) = decimal(&value[5..7]) else {
        return false;
    };
    let Some(day) = decimal(&value[8..10]) else {
        return false;
    };
    let maximum = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap_year(year) => 29,
        2 => 28,
        _ => return false,
    };
    (1..=maximum).contains(&day)
}

fn valid_date_time(value: &str) -> bool {
    if !value.is_ascii() {
        return false;
    }
    let Some((date, time_and_zone)) = value.split_once('T') else {
        return false;
    };
    if !valid_date(date) {
        return false;
    }
    let (time, zone) = if let Some(time) = time_and_zone.strip_suffix('Z') {
        (time, None)
    } else {
        let Some(index) = time_and_zone
            .char_indices()
            .rev()
            .find_map(|(index, character)| matches!(character, '+' | '-').then_some(index))
        else {
            return false;
        };
        (&time_and_zone[..index], Some(&time_and_zone[index + 1..]))
    };
    let mut parts = time.split(':');
    let (Some(hour), Some(minute), Some(second), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    let second = second.split_once('.').map_or(second, |(whole, fraction)| {
        if fraction.is_empty() || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
            "invalid"
        } else {
            whole
        }
    });
    let valid_time = decimal(hour).is_some_and(|value| value <= 23)
        && decimal(minute).is_some_and(|value| value <= 59)
        && decimal(second).is_some_and(|value| value <= 60);
    valid_time && zone.is_none_or(valid_zone)
}

fn valid_zone(value: &str) -> bool {
    value.is_ascii()
        && value.len() == 5
        && &value[2..3] == ":"
        && decimal(&value[..2]).is_some_and(|hour| hour <= 23)
        && decimal(&value[3..]).is_some_and(|minute| minute <= 59)
}

fn decimal(value: &str) -> Option<u32> {
    (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| value.parse().ok())
        .flatten()
}

const fn leap_year(year: u32) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

fn data_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::DataType, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_computed_fields_in_dependency_order() {
        let schema = BTreeMap::from([
            (
                "label".to_owned(),
                DataField {
                    data_type: DataType::String,
                    nullable: false,
                    computed: Some("data.name + data.suffix".to_owned()),
                },
            ),
            (
                "suffix".to_owned(),
                DataField {
                    data_type: DataType::String,
                    nullable: false,
                    computed: Some("\"!\"".to_owned()),
                },
            ),
            (
                "name".to_owned(),
                DataField {
                    data_type: DataType::String,
                    nullable: false,
                    computed: None,
                },
            ),
        ]);
        let input = DataValue::Object(BTreeMap::from([(
            "name".to_owned(),
            DataValue::String("Ada".to_owned()),
        )]));
        let output = resolve_computed_fields(&schema, &input, 32).unwrap();
        assert_eq!(
            output.get_path("label"),
            Some(&DataValue::String("Ada!".to_owned()))
        );
    }

    #[test]
    fn rejects_computed_field_cycles() {
        let schema = BTreeMap::from([
            (
                "a".to_owned(),
                DataField {
                    data_type: DataType::String,
                    nullable: false,
                    computed: Some("b".to_owned()),
                },
            ),
            (
                "b".to_owned(),
                DataField {
                    data_type: DataType::String,
                    nullable: false,
                    computed: Some("a".to_owned()),
                },
            ),
        ]);
        let error =
            resolve_computed_fields(&schema, &DataValue::Object(BTreeMap::new()), 8).unwrap_err();
        assert_eq!(error.code(), ErrorCode::DataCycle);
    }

    #[test]
    fn validates_date_and_date_time_values_without_locale_rules() {
        DataValue::Array(vec![
            DataValue::Date("2024-02-29".to_owned()),
            DataValue::DateTime("2026-08-30T12:34:56.123+02:00".to_owned()),
            DataValue::DateTime("2026-08-30T10:34:56Z".to_owned()),
        ])
        .validate(4, 8, 64)
        .unwrap();
        assert!(DataValue::Date("2023-02-29".to_owned())
            .validate(1, 2, 64)
            .is_err());
        assert!(DataValue::DateTime("2026-08-30 10:34:56".to_owned())
            .validate(1, 2, 64)
            .is_err());
    }
}
