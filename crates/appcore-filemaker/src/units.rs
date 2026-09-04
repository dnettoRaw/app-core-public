// =============================================================================
//        #######
//     ###       ###     F: units.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded units contracts and behavior for this crate.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{ErrorCode, FileMakerError, Result};

/// One millionth of a typographic point, used as geometry truth.
#[derive(Clone, Copy, Default, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct Unit(i64);

impl Unit {
    /// Fixed-point scale per point.
    pub const PER_POINT: i64 = 1_000_000;
    /// Zero length.
    pub const ZERO: Self = Self(0);

    /// Creates units from the raw fixed-point representation.
    #[must_use]
    pub const fn from_raw(raw: i64) -> Self {
        Self(raw)
    }

    /// Returns the raw fixed-point representation.
    #[must_use]
    pub const fn raw(self) -> i64 {
        self.0
    }

    /// Creates an exact integer point measurement.
    pub fn points(points: i64) -> Result<Self> {
        points
            .checked_mul(Self::PER_POINT)
            .map(Self)
            .ok_or_else(|| invalid_unit("point conversion overflow"))
    }

    /// Converts a rational point measurement using half-away-from-zero rounding.
    pub fn from_ratio(numerator: i128, denominator: i128) -> Result<Self> {
        if denominator <= 0 {
            return Err(invalid_unit("unit denominator must be positive"));
        }
        let scaled = numerator
            .checked_mul(i128::from(Self::PER_POINT))
            .ok_or_else(|| invalid_unit("unit conversion overflow"))?;
        let adjustment = denominator / 2;
        let rounded = if scaled >= 0 {
            scaled.checked_add(adjustment)
        } else {
            scaled.checked_sub(adjustment)
        }
        .ok_or_else(|| invalid_unit("unit rounding overflow"))?
            / denominator;
        i64::try_from(rounded)
            .map(Self)
            .map_err(|_| invalid_unit("unit is outside the supported range"))
    }

    /// Checked addition.
    pub fn checked_add(self, other: Self) -> Result<Self> {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or_else(|| invalid_unit("geometry addition overflow"))
    }

    /// Checked subtraction.
    pub fn checked_sub(self, other: Self) -> Result<Self> {
        self.0
            .checked_sub(other.0)
            .map(Self)
            .ok_or_else(|| invalid_unit("geometry subtraction overflow"))
    }

    /// Checked multiplication by a fixed-point millionth ratio.
    pub fn checked_scale(self, millionths: i64) -> Result<Self> {
        let product = i128::from(self.0)
            .checked_mul(i128::from(millionths))
            .ok_or_else(|| invalid_unit("geometry scale overflow"))?;
        let rounded = if product >= 0 {
            product + 500_000
        } else {
            product - 500_000
        } / 1_000_000;
        i64::try_from(rounded)
            .map(Self)
            .map_err(|_| invalid_unit("scaled geometry is outside the supported range"))
    }

    /// Returns a floating-point value for output APIs only.
    #[must_use]
    pub fn as_points_f64(self) -> f64 {
        self.0 as f64 / Self::PER_POINT as f64
    }
}

impl fmt::Debug for Unit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}pt", self.as_points_f64())
    }
}

impl Serialize for Unit {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_i64(self.0)
    }
}

impl<'de> Deserialize<'de> for Unit {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        i64::deserialize(deserializer).map(Self)
    }
}

/// A source length resolved against explicit layout context.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Length {
    /// Absolute fixed-point length.
    Absolute(Unit),
    /// Parts per million of the containing dimension, parsed from `%` or a
    /// bounded `0..=1` `norm`/`normalized` source spelling.
    Percent(i64),
    /// Logical units interpreted by caller-selected context.
    Logical(i64),
    /// Automatic measurement.
    Auto,
}

impl Length {
    /// Resolves the value using explicit percentage and logical bases.
    pub fn resolve(self, percent_base: Unit, logical_unit: Unit) -> Result<Option<Unit>> {
        match self {
            Self::Absolute(value) => Ok(Some(value)),
            Self::Percent(value) => percent_base.checked_scale(value).map(Some),
            Self::Logical(value) => logical_unit.checked_scale(value).map(Some),
            Self::Auto => Ok(None),
        }
    }
}

impl FromStr for Length {
    type Err = FileMakerError;

    fn from_str(value: &str) -> Result<Self> {
        let value = value.trim();
        if value == "auto" {
            return Ok(Self::Auto);
        }
        if let Some(raw) = value.strip_suffix("logical-ppm") {
            return raw
                .parse::<i64>()
                .map(Self::Logical)
                .map_err(|_| invalid_unit("invalid logical fixed-point length"));
        }
        if let Some(raw) = value.strip_suffix("ppm") {
            return raw
                .parse::<i64>()
                .map(Self::Percent)
                .map_err(|_| invalid_unit("invalid percentage fixed-point length"));
        }
        if let Some(raw) = value.strip_suffix("raw") {
            return raw
                .parse::<i64>()
                .map(Unit::from_raw)
                .map(Self::Absolute)
                .map_err(|_| invalid_unit("invalid raw fixed-point length"));
        }
        let (number, suffix) = split_number_suffix(value)?;
        let (numerator, decimal_scale) = parse_decimal(number)?;
        let absolute = match suffix {
            "pt" => Unit::from_ratio(numerator, decimal_scale)?,
            "px" => Unit::from_ratio(numerator * 3, decimal_scale * 4)?,
            "in" => Unit::from_ratio(numerator * 72, decimal_scale)?,
            "mm" => Unit::from_ratio(numerator * 360, decimal_scale * 127)?,
            "cm" => Unit::from_ratio(numerator * 3_600, decimal_scale * 127)?,
            "%" => {
                let ppm = rounded_i64(numerator * 10_000, decimal_scale)?;
                return Ok(Self::Percent(ppm));
            }
            "norm" | "normalized" => {
                let ppm = rounded_i64(numerator * 1_000_000, decimal_scale)?;
                if !(0..=1_000_000).contains(&ppm) {
                    return Err(invalid_unit("normalized length must be between 0 and 1"));
                }
                return Ok(Self::Percent(ppm));
            }
            "lu" | "logical" => {
                let logical = rounded_i64(numerator * 1_000_000, decimal_scale)?;
                return Ok(Self::Logical(logical));
            }
            _ => return Err(invalid_unit(format!("unsupported unit suffix `{suffix}`"))),
        };
        Ok(Self::Absolute(absolute))
    }
}

impl Serialize for Length {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Absolute(value) => serializer.serialize_str(&format!("{}raw", value.raw())),
            Self::Percent(value) => serializer.serialize_str(&format!("{value}ppm")),
            Self::Logical(value) => serializer.serialize_str(&format!("{value}logical-ppm")),
            Self::Auto => serializer.serialize_str("auto"),
        }
    }
}

impl<'de> Deserialize<'de> for Length {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

fn split_number_suffix(value: &str) -> Result<(&str, &str)> {
    let split = value
        .find(|character: char| character.is_ascii_alphabetic() || character == '%')
        .ok_or_else(|| invalid_unit("length requires an explicit unit"))?;
    let (number, suffix) = value.split_at(split);
    if number.is_empty() || suffix.is_empty() {
        return Err(invalid_unit("length requires a number and unit"));
    }
    Ok((number, suffix))
}

fn parse_decimal(value: &str) -> Result<(i128, i128)> {
    let negative = value.starts_with('-');
    let unsigned = value.strip_prefix(['-', '+']).unwrap_or(value);
    let mut parts = unsigned.split('.');
    let whole = parts.next().unwrap_or_default();
    let fractional = parts.next();
    if parts.next().is_some()
        || whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fractional.is_some_and(|part| !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err(invalid_unit("invalid decimal length"));
    }
    let fractional = fractional.unwrap_or_default();
    let scale = 10_i128
        .checked_pow(u32::try_from(fractional.len()).map_err(|_| invalid_unit("decimal too long"))?)
        .ok_or_else(|| invalid_unit("decimal precision overflow"))?;
    let digits = format!("{whole}{fractional}")
        .parse::<i128>()
        .map_err(|_| invalid_unit("decimal length overflow"))?;
    Ok((if negative { -digits } else { digits }, scale))
}

fn rounded_i64(numerator: i128, denominator: i128) -> Result<i64> {
    let adjusted = if numerator >= 0 {
        numerator + denominator / 2
    } else {
        numerator - denominator / 2
    };
    i64::try_from(adjusted / denominator).map_err(|_| invalid_unit("ratio overflow"))
}

fn invalid_unit(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::GeometryInvalid, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_physical_units_deterministically() {
        assert_eq!(
            "1in".parse::<Length>().unwrap(),
            Length::Absolute(Unit::points(72).unwrap())
        );
        assert_eq!(
            "25.4mm".parse::<Length>().unwrap(),
            Length::Absolute(Unit::points(72).unwrap())
        );
        assert_eq!(
            "96px".parse::<Length>().unwrap(),
            Length::Absolute(Unit::points(72).unwrap())
        );
    }

    #[test]
    fn rejects_implicit_units() {
        assert_eq!(
            "12".parse::<Length>().unwrap_err().code(),
            ErrorCode::GeometryInvalid
        );
    }

    #[test]
    fn normalized_coordinates_resolve_against_the_percentage_context() {
        let quarter: Length = "0.25normalized".parse().unwrap();
        assert_eq!(quarter, Length::Percent(250_000));
        assert_eq!(
            quarter
                .resolve(Unit::points(200).unwrap(), Unit::points(1).unwrap())
                .unwrap(),
            Some(Unit::points(50).unwrap())
        );
        assert_eq!(
            "1.1norm".parse::<Length>().unwrap_err().code(),
            ErrorCode::GeometryInvalid
        );
    }
}
