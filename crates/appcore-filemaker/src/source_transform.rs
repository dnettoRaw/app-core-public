// =============================================================================
//        #######
//     ###       ###     F: source_transform.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use serde::{Deserialize, Serialize};

use crate::{ErrorCode, FileMakerError, Length, Result, TransformIr, Unit};

/// Declarative fixed-point transform resolved around an element-local origin.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TransformSource {
    /// Horizontal translation; percentages use element width.
    pub translate_x: Length,
    /// Vertical translation; percentages use element height.
    pub translate_y: Length,
    /// Clockwise rotation in integer degrees.
    pub rotate: i32,
    /// Horizontal scale in millionths (`1_000_000` is 100%).
    pub scale_x: i64,
    /// Vertical scale in millionths (`1_000_000` is 100%).
    pub scale_y: i64,
    /// Explicit horizontal flip.
    pub flip_x: bool,
    /// Explicit vertical flip.
    pub flip_y: bool,
    /// Mirror shorthand composed with the flip flags.
    pub mirror: MirrorSource,
    /// Horizontal transform origin; defaults to `50%`.
    pub origin_x: Length,
    /// Vertical transform origin; defaults to `50%`.
    pub origin_y: Length,
}

impl Default for TransformSource {
    fn default() -> Self {
        Self {
            translate_x: Length::Absolute(Unit::ZERO),
            translate_y: Length::Absolute(Unit::ZERO),
            rotate: 0,
            scale_x: 1_000_000,
            scale_y: 1_000_000,
            flip_x: false,
            flip_y: false,
            mirror: MirrorSource::None,
            origin_x: Length::Percent(500_000),
            origin_y: Length::Percent(500_000),
        }
    }
}

/// Coordinate mirrored by [`TransformSource`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MirrorSource {
    /// No mirror shorthand.
    #[default]
    None,
    /// Mirror horizontal coordinates.
    Horizontal,
    /// Mirror vertical coordinates.
    Vertical,
    /// Mirror both coordinates.
    Both,
}

pub(crate) fn validate_transform(source: &TransformSource) -> Result<()> {
    const MAX_SCALE: i64 = 100_000_000;
    if source.scale_x == 0
        || source.scale_y == 0
        || source.scale_x.unsigned_abs() > MAX_SCALE as u64
        || source.scale_y.unsigned_abs() > MAX_SCALE as u64
        || matches!(source.origin_x, Length::Auto)
        || matches!(source.origin_y, Length::Auto)
        || matches!(source.translate_x, Length::Auto)
        || matches!(source.translate_y, Length::Auto)
    {
        return Err(FileMakerError::new(
            ErrorCode::SchemaField,
            "transform requires non-zero bounded scales and explicit translations/origins",
        ));
    }
    Ok(())
}

pub(crate) fn convert_transform(source: TransformSource) -> Result<TransformIr> {
    let mirror_x = matches!(source.mirror, MirrorSource::Horizontal | MirrorSource::Both);
    let mirror_y = matches!(source.mirror, MirrorSource::Vertical | MirrorSource::Both);
    let scale_x = signed_scale(source.scale_x, source.flip_x ^ mirror_x, "horizontal")?;
    let scale_y = signed_scale(source.scale_y, source.flip_y ^ mirror_y, "vertical")?;
    Ok(TransformIr {
        translate_x: source.translate_x,
        translate_y: source.translate_y,
        rotate: source.rotate,
        scale_x,
        scale_y,
        origin_x: source.origin_x,
        origin_y: source.origin_y,
    })
}

fn signed_scale(scale: i64, flip: bool, axis: &str) -> Result<i64> {
    if flip {
        scale.checked_neg().ok_or_else(|| {
            FileMakerError::new(ErrorCode::GeometryInvalid, format!("{axis} scale overflow"))
        })
    } else {
        Ok(scale)
    }
}
