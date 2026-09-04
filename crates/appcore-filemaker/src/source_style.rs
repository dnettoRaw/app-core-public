// =============================================================================
//        #######
//     ###       ###     F: source_style.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use crate::source::{ColorSource, StyleSource};
use crate::{Color, ErrorCode, FileMakerError, Length, Result, Style, Unit};

pub(crate) fn convert_style(source: &StyleSource) -> Result<Style> {
    let absolute = |length: Option<Length>, label: &str| -> Result<Option<Unit>> {
        length
            .map(|value| {
                value.resolve(Unit::ZERO, Unit::ZERO)?.ok_or_else(|| {
                    FileMakerError::new(ErrorCode::SchemaField, format!("{label} must be absolute"))
                })
            })
            .transpose()
    };
    let style = Style {
        fill: source.fill.as_ref().map(convert_color).transpose()?,
        stroke: source.stroke.as_ref().map(convert_color).transpose()?,
        stroke_width: absolute(source.stroke_width, "stroke width")?,
        opacity: source.opacity,
        font: source.font.clone(),
        font_size: absolute(source.font_size, "font size")?,
        color: source.color.as_ref().map(convert_color).transpose()?,
    };
    style.validate()?;
    Ok(style)
}

fn convert_color(source: &ColorSource) -> Result<Color> {
    let color = match source {
        ColorSource::Text(source) => Color::parse(source)?,
        ColorSource::Typed(color) => *color,
    };
    color.validate()?;
    Ok(color)
}
