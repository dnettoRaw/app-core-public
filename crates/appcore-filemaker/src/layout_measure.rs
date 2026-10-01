// =============================================================================
//        #######
//     ###       ###     F: layout_measure.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Measures element content and applies its resolved style and text options.
//!
//! Measurement is completed before collision placement; the returned bounds
//! are the shared input for layout and every visual exporter.

use crate::{
    resolve_image_placement, AssetResolver, ComputedStyle, ElementIr, ElementKind, ErrorCode,
    FileMakerError, FontManager, ImagePlacement, Rect, ResourceLimits, Result, StyleCascade,
    TextEngine, TextLayout, TextOptions, Unit, WritingMode,
};

pub(crate) fn measure_content(
    element: &ElementIr,
    bounds: Rect,
    fonts: &FontManager,
    logical_unit: Unit,
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
    let padding_inline = element
        .text_options
        .padding_inline
        .resolve(bounds.size.width, logical_unit)?
        .ok_or_else(|| {
            FileMakerError::new(
                ErrorCode::LayoutInvalid,
                "text inline padding cannot be auto",
            )
        })?;
    let padding = element.text_options.padding.resolve(
        bounds.size.width,
        bounds.size.height,
        logical_unit,
    )?;
    let content_width = bounds
        .size
        .width
        .checked_sub(padding.left.checked_add(padding.right)?)?;
    let content_height = bounds
        .size
        .height
        .checked_sub(padding.top.checked_add(padding.bottom)?)?;
    if content_width <= Unit::ZERO || content_height <= Unit::ZERO {
        return Err(FileMakerError::new(
            ErrorCode::LayoutInvalid,
            "text block padding leaves no content area",
        ));
    }
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
        bounds: crate::Size::new(content_width, content_height)?,
        max_lines: element.text_options.max_lines,
        overflow: element.text_options.overflow,
        line_height: style
            .line_height
            .unwrap_or(element.text_options.line_height),
        writing_mode: element.text_options.writing_mode,
        align_x: element.text_options.align_x,
        padding_inline,
    };
    let mut text_layout = if element.text_segments.is_empty() {
        TextEngine::new(fonts).layout(element.text.as_deref().unwrap_or_default(), &options)?
    } else {
        layout_text_segments(element, &options, fonts, logical_unit)?
    };
    text_layout.padding = padding;
    let (natural_width, natural_height) = match text_layout.writing_mode {
        WritingMode::Horizontal => (
            text_layout
                .lines
                .iter()
                .map(|line| line.width)
                .max()
                .unwrap_or(Unit::ZERO)
                .checked_add(padding_inline.checked_scale(2_000_000)?)?
                .checked_add(padding.left.checked_add(padding.right)?)?,
            sum_block_advances(&text_layout)?
                .checked_add(padding.top.checked_add(padding.bottom)?)?,
        ),
        WritingMode::Vertical => (
            sum_block_advances(&text_layout)?
                .checked_add(padding.left.checked_add(padding.right)?)?,
            text_layout
                .lines
                .iter()
                .map(|line| line.width)
                .max()
                .unwrap_or(Unit::ZERO)
                .checked_add(padding_inline.checked_scale(2_000_000)?)?
                .checked_add(padding.top.checked_add(padding.bottom)?)?,
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

fn layout_text_segments(
    element: &ElementIr,
    options: &TextOptions,
    fonts: &FontManager,
    logical_unit: Unit,
) -> Result<TextLayout> {
    let engine = TextEngine::new(fonts);
    let mut runs = Vec::new();
    let mut source_text = String::new();
    let mut width = Unit::ZERO;
    let mut height = options
        .font_size
        .checked_scale(i64::from(options.line_height))?;
    for segment in &element.text_segments {
        let text = segment.text.as_deref().ok_or_else(|| {
            FileMakerError::new(ErrorCode::DataType, "inline text segment was not bound")
        })?;
        if text.contains('\n') || text.contains('\r') {
            return Err(FileMakerError::new(
                ErrorCode::DataType,
                "inline text segment binding must resolve to one line",
            ));
        }
        if !text.is_empty() {
            let layout = engine.layout(text, options)?;
            let line = layout.lines.first().ok_or_else(|| {
                FileMakerError::new(ErrorCode::LayoutInvalid, "inline text segment has no line")
            })?;
            if layout.lines.len() != 1 {
                return Err(FileMakerError::new(
                    ErrorCode::LayoutInvalid,
                    "inline text segments cannot wrap independently",
                ));
            }
            if line.runs.iter().any(|run| run.rtl) {
                return Err(FileMakerError::new(
                    ErrorCode::LayoutInvalid,
                    "inline text segments do not support bidirectional composition",
                ));
            }
            width = width.checked_add(line.width)?;
            height = height.max(line.height);
            runs.extend(line.runs.iter().cloned());
            source_text.push_str(text);
        }
        let gap = segment
            .gap_after
            .resolve(options.bounds.width, logical_unit)?
            .ok_or_else(|| {
                FileMakerError::new(ErrorCode::LayoutInvalid, "inline text gap cannot be auto")
            })?;
        if gap > Unit::ZERO {
            width = width.checked_add(gap)?;
            runs.push(crate::GlyphRun {
                font: options.font.clone(),
                rtl: false,
                text: String::new(),
                glyphs: Vec::new(),
                width: gap,
            });
        }
    }
    if width > options.bounds.width {
        return Err(FileMakerError::new(
            ErrorCode::LayoutInvalid,
            "inline text segments exceed their one-line content width",
        ));
    }
    let line = crate::TextLine {
        source_text,
        runs,
        width,
        height,
    };
    Ok(TextLayout {
        writing_mode: WritingMode::Horizontal,
        lines: vec![line],
        measured: crate::Size::new(options.bounds.width, options.bounds.height)?,
        font_size: options.font_size,
        paint_offset_y: Unit::ZERO,
        diagnostics: Vec::new(),
        align_x: options.align_x,
        padding_inline: options.padding_inline,
        padding: crate::Insets::default(),
    })
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
