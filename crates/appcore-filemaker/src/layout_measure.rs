// =============================================================================
//        #######
//     ###       ###     F: layout_measure.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use crate::{
    resolve_image_placement, AssetResolver, ComputedStyle, ElementIr, ElementKind, ErrorCode,
    FileMakerError, FontManager, ImagePlacement, Rect, ResourceLimits, Result, StyleCascade,
    TextEngine, TextLayout, TextOptions, Unit, WritingMode,
};

pub(crate) fn measure_content(
    element: &ElementIr,
    bounds: Rect,
    fonts: &FontManager,
) -> Result<(ComputedStyle, Option<TextLayout>, Rect)> {
    let style = StyleCascade {
        template: element.style.clone(),
        ..StyleCascade::default()
    }
    .compute()?;
    if element.kind != ElementKind::Text {
        return Ok((style, None, bounds));
    }
    let font = style.font.clone().ok_or_else(|| {
        FileMakerError::new(ErrorCode::FontMissing, "text requires an explicit font")
    })?;
    let options = TextOptions {
        font,
        font_size: style.font_size,
        min_font_size: element.text_options.min_font_size.map_or(
            Ok(Unit::from_raw(6_000_000)),
            |value| {
                value.resolve(Unit::ZERO, Unit::ZERO)?.ok_or_else(|| {
                    FileMakerError::new(
                        ErrorCode::LayoutInvalid,
                        "minimum font size cannot be auto",
                    )
                })
            },
        )?,
        bounds: bounds.size,
        max_lines: element.text_options.max_lines,
        overflow: element.text_options.overflow,
        line_height: element.text_options.line_height,
        writing_mode: element.text_options.writing_mode,
    };
    let text_layout =
        TextEngine::new(fonts).layout(element.text.as_deref().unwrap_or_default(), &options)?;
    let (natural_width, natural_height) = match text_layout.writing_mode {
        WritingMode::Horizontal => (
            text_layout
                .lines
                .iter()
                .map(|line| line.width)
                .max()
                .unwrap_or(Unit::ZERO),
            sum_block_advances(&text_layout)?,
        ),
        WritingMode::Vertical => (
            sum_block_advances(&text_layout)?,
            text_layout
                .lines
                .iter()
                .map(|line| line.width)
                .max()
                .unwrap_or(Unit::ZERO),
        ),
    };
    let intrinsic = Rect::new(
        bounds.origin.x,
        bounds.origin.y,
        natural_width,
        natural_height,
    )?;
    Ok((style, Some(text_layout), intrinsic))
}

fn sum_block_advances(layout: &TextLayout) -> Result<Unit> {
    layout
        .lines
        .iter()
        .try_fold(Unit::ZERO, |total, line| total.checked_add(line.height))
}

pub(crate) fn resolve_image(
    element: &ElementIr,
    bounds: Rect,
    resolver: Option<&dyn AssetResolver>,
    limits: &ResourceLimits,
) -> Result<Option<ImagePlacement>> {
    if element.kind != ElementKind::Image {
        return Ok(None);
    }
    let (Some(name), Some(resolver)) = (&element.asset, resolver) else {
        return Ok(None);
    };
    let asset = resolver.resolve_asset(name, limits.max_asset_bytes)?;
    resolve_image_placement(&asset, bounds, element.image, limits.max_pixels).map(Some)
}
