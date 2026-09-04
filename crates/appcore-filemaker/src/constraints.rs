// =============================================================================
//        #######
//     ###       ###     F: constraints.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded constraints contracts and behavior for this crate.

use serde::{Deserialize, Serialize};

use crate::{ErrorCode, FileMakerError, Length, Result, Size, Unit};

/// Alignment of an element inside its resolved container.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Alignment {
    /// Align to the container's leading edge.
    Start,
    /// Center on the selected axis.
    Center,
    /// Align to the container's trailing edge.
    End,
}

/// Distribution of fixed-size children along a flow's primary axis.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Distribution {
    /// Start at the leading edge and retain the declared gap.
    #[default]
    Start,
    /// Center the complete flow.
    Center,
    /// Place the complete flow at the trailing edge.
    End,
    /// Distribute remaining space only between children.
    SpaceBetween,
    /// Distribute remaining space around children using half edge spaces.
    SpaceAround,
    /// Distribute equal space at edges and between children.
    SpaceEvenly,
}

/// Bounded size intent applied before measurement and collision.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LayoutConstraints {
    /// Minimum width.
    pub min_width: Option<Length>,
    /// Preferred width used when `width` is absent.
    pub preferred_width: Option<Length>,
    /// Maximum width.
    pub max_width: Option<Length>,
    /// Minimum height.
    pub min_height: Option<Length>,
    /// Preferred height used when `height` is absent.
    pub preferred_height: Option<Length>,
    /// Maximum height.
    pub max_height: Option<Length>,
    /// Width divided by height, in millionths.
    pub aspect_ratio: Option<u32>,
}

impl LayoutConstraints {
    pub(crate) fn validate(&self) -> Result<()> {
        if self.aspect_ratio == Some(0)
            || [
                self.min_width,
                self.preferred_width,
                self.max_width,
                self.min_height,
                self.preferred_height,
                self.max_height,
            ]
            .into_iter()
            .flatten()
            .any(|length| matches!(length, Length::Auto))
        {
            return Err(constraint_error(
                "layout constraints require concrete lengths and a positive aspect ratio",
            ));
        }
        Ok(())
    }
}

pub(crate) fn resolve_constrained_size(
    explicit_width: Option<Length>,
    explicit_height: Option<Length>,
    constraints: LayoutConstraints,
    container: Size,
    default_size: Size,
    logical_unit: Unit,
) -> Result<Size> {
    constraints.validate()?;
    let width_intent = resolve_optional(explicit_width, container.width, logical_unit)?.or(
        resolve_optional(constraints.preferred_width, container.width, logical_unit)?,
    );
    let height_intent = resolve_optional(explicit_height, container.height, logical_unit)?.or(
        resolve_optional(constraints.preferred_height, container.height, logical_unit)?,
    );
    let width_range = resolve_range(
        constraints.min_width,
        constraints.max_width,
        container.width,
        logical_unit,
    )?;
    let height_range = resolve_range(
        constraints.min_height,
        constraints.max_height,
        container.height,
        logical_unit,
    )?;
    let mut width = clamp(width_intent.unwrap_or(default_size.width), width_range)?;
    let mut height = clamp(height_intent.unwrap_or(default_size.height), height_range)?;
    if let Some(ratio) = constraints.aspect_ratio {
        match (width_intent, height_intent) {
            (Some(_), Some(_)) => validate_ratio(width, height, ratio)?,
            (None, Some(_)) => {
                width = clamp(height.checked_scale(i64::from(ratio))?, width_range)?;
                validate_ratio(width, height, ratio)?;
            }
            (Some(_), None) | (None, None) => {
                height = clamp(
                    Unit::from_ratio(i128::from(width.raw()), i128::from(ratio))?,
                    height_range,
                )?;
                validate_ratio(width, height, ratio)?;
            }
        }
    }
    Size::new(width, height)
}

fn resolve_optional(
    value: Option<Length>,
    percent_base: Unit,
    logical_unit: Unit,
) -> Result<Option<Unit>> {
    value.map_or(Ok(None), |length| {
        length.resolve(percent_base, logical_unit)
    })
}

fn resolve_range(
    minimum: Option<Length>,
    maximum: Option<Length>,
    percent_base: Unit,
    logical_unit: Unit,
) -> Result<(Option<Unit>, Option<Unit>)> {
    let minimum = resolve_optional(minimum, percent_base, logical_unit)?;
    let maximum = resolve_optional(maximum, percent_base, logical_unit)?;
    if minimum.zip(maximum).is_some_and(|(min, max)| min > max) {
        return Err(constraint_error(
            "minimum layout constraint exceeds maximum",
        ));
    }
    Ok((minimum, maximum))
}

fn clamp(value: Unit, range: (Option<Unit>, Option<Unit>)) -> Result<Unit> {
    let value = range.0.map_or(value, |minimum| value.max(minimum));
    let value = range.1.map_or(value, |maximum| value.min(maximum));
    if value < Unit::ZERO {
        return Err(constraint_error(
            "resolved constraint size cannot be negative",
        ));
    }
    Ok(value)
}

fn validate_ratio(width: Unit, height: Unit, ratio: u32) -> Result<()> {
    if height <= Unit::ZERO {
        return Err(constraint_error("aspect ratio requires positive height"));
    }
    let expected = height.checked_scale(i64::from(ratio))?;
    if width.raw().abs_diff(expected.raw()) > 1 {
        return Err(constraint_error(
            "resolved min/max constraints conflict with aspect ratio",
        ));
    }
    Ok(())
}

fn constraint_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::LayoutInvalid, message)
}
