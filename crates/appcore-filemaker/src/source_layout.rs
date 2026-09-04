// =============================================================================
//        #######
//     ###       ###     F: source_layout.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded source layout contracts and behavior for this crate.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    Distribution, ElementSource, ErrorCode, FileMakerError, LayoutMode, Length, Result, Unit,
};

/// Named non-painted geometry inserted into every page collision index.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExclusionSource {
    /// Page-relative horizontal coordinate.
    pub x: Length,
    /// Page-relative vertical coordinate.
    pub y: Length,
    /// Page-relative width.
    pub width: Length,
    /// Page-relative height.
    pub height: Length,
    /// Collision group exposed by the exclusion.
    #[serde(default = "default_exclusion_group")]
    pub group: String,
    /// Candidate groups blocked by this exclusion; empty means every group.
    #[serde(default)]
    pub collides_with: Vec<String>,
}

pub(crate) fn validate_exclusions(
    exclusions: &BTreeMap<String, ExclusionSource>,
    max_exclusions: usize,
) -> Result<()> {
    if exclusions.len() > max_exclusions {
        return Err(limit_error(
            "exclusion count exceeds configured element limit",
        ));
    }
    for (name, exclusion) in exclusions {
        validate_name("exclusion", name, 118)?;
        validate_name("exclusion group", &exclusion.group, 128)?;
        if exclusion.collides_with.len() > 64 {
            return Err(limit_error("exclusion collision-group list exceeds 64"));
        }
        for group in &exclusion.collides_with {
            validate_name("exclusion collision group", group, 128)?;
        }
        if [exclusion.x, exclusion.y, exclusion.width, exclusion.height].contains(&Length::Auto) {
            return Err(schema_error("exclusion geometry cannot be auto"));
        }
    }
    Ok(())
}

pub(crate) fn convert_exclusions(
    exclusions: &BTreeMap<String, ExclusionSource>,
) -> BTreeMap<String, crate::ExclusionIr> {
    exclusions
        .iter()
        .map(|(name, source)| {
            (
                name.clone(),
                crate::ExclusionIr {
                    x: source.x,
                    y: source.y,
                    width: source.width,
                    height: source.height,
                    group: source.group.clone(),
                    collides_with: source
                        .collides_with
                        .iter()
                        .cloned()
                        .collect::<BTreeSet<_>>(),
                },
            )
        })
        .collect()
}

fn default_exclusion_group() -> String {
    "exclusion".to_owned()
}

fn validate_name(label: &str, value: &str, max_bytes: usize) -> Result<()> {
    if value.is_empty()
        || value.len() > max_bytes
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(schema_error(format!("{label} name is invalid")));
    }
    Ok(())
}

fn schema_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::SchemaField, message)
}

fn limit_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::LimitExceeded, message)
}

pub(crate) fn default_gap() -> Length {
    Length::Absolute(Unit::ZERO)
}

pub(crate) fn validate_layout_source(element: &ElementSource) -> Result<()> {
    element.constraints.validate().map_err(|error| {
        FileMakerError::new(ErrorCode::SchemaField, error.message()).at(element.id.clone())
    })?;
    if element.align_x.is_some() && element.x.is_some()
        || element.align_y.is_some() && element.y.is_some()
    {
        return Err(FileMakerError::new(
            ErrorCode::SchemaField,
            "aligned axes cannot also declare an explicit coordinate",
        )
        .at(element.id.clone()));
    }
    if element.distribute != Distribution::Start && element.layout == LayoutMode::Absolute {
        return Err(FileMakerError::new(
            ErrorCode::SchemaField,
            "distribution requires flow_vertical or flow_horizontal layout",
        )
        .at(element.id.clone()));
    }
    Ok(())
}
