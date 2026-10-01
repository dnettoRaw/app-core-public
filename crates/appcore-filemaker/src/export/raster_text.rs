// =============================================================================
//        #######
//     ###       ###     F: raster_text.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Paints already-shaped glyph outlines without measuring or reflowing text.

use skrifa::{
    instance::{LocationRef, Size},
    outline::DrawSettings,
    FontRef, GlyphId, MetadataProvider,
};
use tiny_skia::{FillRule, PathBuilder, Pixmap, Stroke, Transform as SkTransform};

use super::raster::{geometry_clip_mask, paint, to_pixel};
use super::raster_outline::TinyOutline;
use crate::{
    ComputedStyle, ErrorCode, ExportContext, FileMakerError, Rect, Result, TextLayout, WritingMode,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn render_layout(
    pixmap: &mut Pixmap,
    layout: &TextLayout,
    bounds: Rect,
    style: &ComputedStyle,
    context: &ExportContext<'_>,
    scale: f32,
    page_y: f32,
    element_transform: SkTransform,
    clip: Option<Rect>,
) -> Result<()> {
    let (mut line_x, mut line_y) = match layout.writing_mode {
        WritingMode::Horizontal => (
            to_pixel(bounds.origin.x, scale),
            to_pixel(bounds.origin.y, scale)
                + to_pixel(layout.font_size, scale)
                + to_pixel(layout.paint_offset_y, scale)
                + page_y,
        ),
        WritingMode::Vertical => (
            to_pixel(bounds.origin.x, scale) + to_pixel(bounds.size.width, scale),
            to_pixel(bounds.origin.y, scale) + to_pixel(layout.paint_offset_y, scale) + page_y,
        ),
    };
    let clip_mask = clip
        .map(|clip| {
            geometry_clip_mask(
                pixmap.width(),
                pixmap.height(),
                clip,
                element_transform,
                scale,
                page_y,
            )
        })
        .transpose()?;
    for line in &layout.lines {
        render_line(
            pixmap,
            line,
            layout,
            bounds,
            style,
            context,
            scale,
            (line_x, line_y),
            element_transform,
            clip_mask.as_ref(),
        )?;
        match layout.writing_mode {
            WritingMode::Horizontal => line_y += to_pixel(line.height, scale),
            WritingMode::Vertical => line_x -= to_pixel(line.height, scale),
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn render_line(
    pixmap: &mut Pixmap,
    line: &crate::TextLine,
    layout: &TextLayout,
    bounds: Rect,
    style: &ComputedStyle,
    context: &ExportContext<'_>,
    scale: f32,
    origin: (f32, f32),
    element_transform: SkTransform,
    clip_mask: Option<&tiny_skia::Mask>,
) -> Result<()> {
    let (line_x, line_y) = origin;
    let inline_size = if layout.writing_mode == WritingMode::Horizontal {
        bounds.size.width
    } else {
        bounds.size.height
    };
    let inline_offset = to_pixel(layout.inline_offset(inline_size, line)?, scale);
    let (mut cursor_x, mut cursor_y) = match layout.writing_mode {
        WritingMode::Horizontal => (line_x + inline_offset, line_y),
        WritingMode::Vertical => (line_x, line_y + inline_offset),
    };
    for run in &line.runs {
        let font = context.fonts.get_outline_asset(&run.font)?;
        let face = FontRef::from_index(&font.bytes, font.face_index)
            .map_err(|_| raster_text_error("cannot parse font during raster export"))?;
        let units_per_em = face
            .metrics(Size::unscaled(), LocationRef::default())
            .units_per_em;
        if units_per_em == 0 {
            return Err(raster_text_error("font has no units-per-em"));
        }
        let font_scale = to_pixel(layout.font_size, scale) / f32::from(units_per_em);
        let outlines = face.outline_glyphs();
        for glyph in &run.glyphs {
            let mut outline = TinyOutline::default();
            if let Some(outline_glyph) = outlines.get(GlyphId::new(u32::from(glyph.id))) {
                outline_glyph
                    .draw(
                        DrawSettings::unhinted(Size::unscaled(), LocationRef::default()),
                        &mut outline,
                    )
                    .map_err(|_| raster_text_error("cannot draw raster font outline"))?;
                if let Some(path) = outline.finish() {
                    let glyph_transform = SkTransform::from_row(
                        font_scale,
                        0.0,
                        0.0,
                        -font_scale,
                        cursor_x + to_pixel(glyph.offset_x, scale),
                        cursor_y - to_pixel(glyph.offset_y, scale),
                    );
                    pixmap.fill_path(
                        &path,
                        &paint(style.color, style.opacity),
                        FillRule::Winding,
                        element_transform.pre_concat(glyph_transform),
                        clip_mask,
                    );
                }
            }
            cursor_x += to_pixel(glyph.advance_x, scale);
            cursor_y -= to_pixel(glyph.advance_y, scale);
        }
    }
    if style.underline && layout.writing_mode == WritingMode::Horizontal {
        render_underline(
            pixmap,
            line,
            layout,
            style,
            scale,
            line_x + inline_offset,
            line_y,
            element_transform,
            clip_mask,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn render_underline(
    pixmap: &mut Pixmap,
    line: &crate::TextLine,
    layout: &TextLayout,
    style: &ComputedStyle,
    scale: f32,
    x: f32,
    line_y: f32,
    element_transform: SkTransform,
    clip_mask: Option<&tiny_skia::Mask>,
) -> Result<()> {
    let width = line
        .runs
        .iter()
        .try_fold(crate::Unit::ZERO, |width, run| width.checked_add(run.width))?;
    if width <= crate::Unit::ZERO {
        return Ok(());
    }
    let y = line_y + to_pixel(layout.font_size.checked_scale(120_000)?, scale);
    let mut builder = PathBuilder::new();
    builder.move_to(x, y);
    builder.line_to(x + to_pixel(width, scale), y);
    if let Some(path) = builder.finish() {
        pixmap.stroke_path(
            &path,
            &paint(style.color, style.opacity),
            &Stroke {
                width: to_pixel(layout.font_size.checked_scale(40_000)?, scale).max(0.5),
                ..Stroke::default()
            },
            element_transform,
            clip_mask,
        );
    }
    Ok(())
}

fn raster_text_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::ExportUnsupported, message)
}
