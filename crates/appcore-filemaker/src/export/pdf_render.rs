// =============================================================================
//        #######
//     ###       ###     F: pdf_render.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded pdf render contracts and behavior for this crate.

use std::collections::BTreeMap;

use pdf_writer::types::TextRenderingMode;
use pdf_writer::{Content, Name, Str};
use skrifa::{
    instance::{LocationRef, Size},
    outline::DrawSettings,
    FontRef, GlyphId, MetadataProvider,
};

use super::pdf_font::{unit, PdfFont};
use super::pdf_image::PdfImage;
use super::pdf_outline::PdfOutline;
use super::pdf_paint::{apply_opacity, effective_opacity, set_fill, set_stroke};
use super::progress::ExportProgress;
use crate::{
    ElementKind, ErrorCode, ExportContext, FileMakerError, PdfMode, ResolvedElement, ResolvedPage,
    Result, Shape, TextLayout, WritingMode,
};

pub(super) fn render_page(
    page: &ResolvedPage,
    mode: PdfMode,
    context: &ExportContext<'_>,
    fonts: &BTreeMap<String, PdfFont>,
    images: &BTreeMap<String, PdfImage>,
    opacities: &BTreeMap<u32, pdf_writer::Ref>,
    mut progress: Option<&mut ExportProgress<'_>>,
) -> Result<Vec<u8>> {
    let mut content = Content::new();
    let height = unit(page.size.height);
    for element in &page.elements {
        content.save_state();
        if !element.transform.is_identity() {
            content.transform(super::pdf_geometry::transform(element.transform, height));
        }
        if let Some(clip) = element.bounds.clip {
            content
                .rect(
                    unit(clip.origin.x),
                    height - unit(clip.origin.y) - unit(clip.size.height),
                    unit(clip.size.width),
                    unit(clip.size.height),
                )
                .clip_nonzero()
                .end_path();
        }
        let rendered = match element.kind {
            ElementKind::Text => render_text(
                &mut content,
                element,
                height,
                mode,
                context,
                fonts,
                opacities,
            ),
            ElementKind::Image => {
                render_image(&mut content, element, height, images);
                Ok(())
            }
            ElementKind::Table => super::table_pdf::render(
                &mut content,
                element,
                height,
                mode,
                context,
                fonts,
                opacities,
            ),
            ElementKind::Chart | ElementKind::Qr | ElementKind::Barcode => Ok(()),
            _ => render_shape(&mut content, element, height, opacities),
        };
        content.restore_state();
        rendered?;
        if let Some(progress) = progress.as_deref_mut() {
            progress.element()?;
        }
    }
    Ok(content.finish().into_vec())
}

fn render_shape(
    content: &mut Content,
    element: &ResolvedElement,
    page_height: f32,
    opacities: &BTreeMap<u32, pdf_writer::Ref>,
) -> Result<()> {
    if let Some(fill) = element.style.fill {
        apply_opacity(
            content,
            effective_opacity(element.style.opacity, fill),
            opacities,
        );
        set_fill(content, fill);
        append_shape(content, element, page_height)?;
        content.fill_nonzero();
    }
    if let Some(stroke) = element.style.stroke {
        apply_opacity(
            content,
            effective_opacity(element.style.opacity, stroke),
            opacities,
        );
        set_stroke(content, stroke);
        content.set_line_width(unit(element.style.stroke_width));
        append_shape(content, element, page_height)?;
        content.stroke();
    }
    Ok(())
}

fn append_shape(content: &mut Content, element: &ResolvedElement, page_height: f32) -> Result<()> {
    let rect = element.bounds.layout;
    let x = unit(rect.origin.x);
    let y = page_height - unit(rect.origin.y) - unit(rect.size.height);
    let width = unit(rect.size.width);
    let height = unit(rect.size.height);
    match &element.shape {
        Shape::Rect { .. } => {
            content.rect(x, y, width, height);
        }
        Shape::Ellipse { .. } => append_ellipse(content, x, y, width, height),
        Shape::Polygon { points } => {
            if let Some(first) = points.first() {
                content.move_to(unit(first.x), page_height - unit(first.y));
                for point in &points[1..] {
                    content.line_to(unit(point.x), page_height - unit(point.y));
                }
                content.close_path();
            }
        }
        Shape::Path { commands, .. } => {
            for command in commands {
                match command {
                    crate::PathCommand::Move { to } => {
                        content.move_to(unit(to.x), page_height - unit(to.y));
                    }
                    crate::PathCommand::Line { to } => {
                        content.line_to(unit(to.x), page_height - unit(to.y));
                    }
                    crate::PathCommand::Curve {
                        control_1,
                        control_2,
                        to,
                    } => {
                        content.cubic_to(
                            unit(control_1.x),
                            page_height - unit(control_1.y),
                            unit(control_2.x),
                            page_height - unit(control_2.y),
                            unit(to.x),
                            page_height - unit(to.y),
                        );
                    }
                    crate::PathCommand::Close => {
                        content.close_path();
                    }
                }
            }
        }
    }
    Ok(())
}

fn append_ellipse(content: &mut Content, x: f32, y: f32, width: f32, height: f32) {
    const KAPPA: f32 = 0.552_284_8;
    let rx = width / 2.0;
    let ry = height / 2.0;
    let cx = x + rx;
    let cy = y + ry;
    content.move_to(cx + rx, cy);
    content.cubic_to(
        cx + rx,
        cy + KAPPA * ry,
        cx + KAPPA * rx,
        cy + ry,
        cx,
        cy + ry,
    );
    content.cubic_to(
        cx - KAPPA * rx,
        cy + ry,
        cx - rx,
        cy + KAPPA * ry,
        cx - rx,
        cy,
    );
    content.cubic_to(
        cx - rx,
        cy - KAPPA * ry,
        cx - KAPPA * rx,
        cy - ry,
        cx,
        cy - ry,
    );
    content.cubic_to(
        cx + KAPPA * rx,
        cy - ry,
        cx + rx,
        cy - KAPPA * ry,
        cx + rx,
        cy,
    );
    content.close_path();
}

fn render_text(
    content: &mut Content,
    element: &ResolvedElement,
    page_height: f32,
    mode: PdfMode,
    context: &ExportContext<'_>,
    fonts: &BTreeMap<String, PdfFont>,
    opacities: &BTreeMap<u32, pdf_writer::Ref>,
) -> Result<()> {
    let layout = element.text_layout.as_ref().ok_or_else(|| {
        FileMakerError::new(
            ErrorCode::ExportWrite,
            "resolved PDF text has no glyph layout",
        )
    })?;
    apply_opacity(
        content,
        effective_opacity(element.style.opacity, element.style.color),
        opacities,
    );
    set_fill(content, element.style.color);
    render_text_layout(
        content,
        layout,
        element.bounds.layout,
        page_height,
        mode,
        context,
        fonts,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_text_layout(
    content: &mut Content,
    layout: &TextLayout,
    bounds: crate::Rect,
    page_height: f32,
    mode: PdfMode,
    context: &ExportContext<'_>,
    fonts: &BTreeMap<String, PdfFont>,
) -> Result<()> {
    let (mut line_x, mut line_y) = match layout.writing_mode {
        WritingMode::Horizontal => (
            unit(bounds.origin.x),
            page_height - unit(bounds.origin.y) - unit(layout.font_size),
        ),
        WritingMode::Vertical => (
            unit(bounds.origin.x) + unit(bounds.size.width),
            page_height - unit(bounds.origin.y),
        ),
    };
    for line in &layout.lines {
        let (mut cursor_x, mut cursor_y) = (line_x, line_y);
        for run in &line.runs {
            match mode {
                PdfMode::Editable => {
                    render_editable_run(content, run, layout.font_size, cursor_x, cursor_y, fonts)?
                }
                PdfMode::Flattened => render_flattened_run(
                    content,
                    run,
                    layout.font_size,
                    cursor_x,
                    cursor_y,
                    context,
                )?,
                PdfMode::Hybrid => {
                    render_flattened_run(
                        content,
                        run,
                        layout.font_size,
                        cursor_x,
                        cursor_y,
                        context,
                    )?;
                    render_invisible_editable_run(
                        content,
                        run,
                        layout.font_size,
                        cursor_x,
                        cursor_y,
                        fonts,
                    )?
                }
            }
            match layout.writing_mode {
                WritingMode::Horizontal => cursor_x += unit(run.width),
                WritingMode::Vertical => cursor_y -= unit(run.width),
            }
        }
        match layout.writing_mode {
            WritingMode::Horizontal => line_y -= unit(line.height),
            WritingMode::Vertical => line_x -= unit(line.height),
        }
    }
    Ok(())
}

pub(super) fn render_editable_run(
    content: &mut Content,
    run: &crate::GlyphRun,
    size: crate::Unit,
    start_x: f32,
    baseline: f32,
    fonts: &BTreeMap<String, PdfFont>,
) -> Result<()> {
    render_editable_run_with_mode(
        content,
        run,
        size,
        start_x,
        baseline,
        fonts,
        TextRenderingMode::Fill,
    )
}

pub(super) fn render_invisible_editable_run(
    content: &mut Content,
    run: &crate::GlyphRun,
    size: crate::Unit,
    start_x: f32,
    baseline: f32,
    fonts: &BTreeMap<String, PdfFont>,
) -> Result<()> {
    render_editable_run_with_mode(
        content,
        run,
        size,
        start_x,
        baseline,
        fonts,
        TextRenderingMode::Invisible,
    )
}

#[allow(clippy::too_many_arguments)]
fn render_editable_run_with_mode(
    content: &mut Content,
    run: &crate::GlyphRun,
    size: crate::Unit,
    start_x: f32,
    baseline: f32,
    fonts: &BTreeMap<String, PdfFont>,
    rendering_mode: TextRenderingMode,
) -> Result<()> {
    let font = fonts
        .get(&run.font)
        .ok_or_else(|| FileMakerError::new(ErrorCode::FontMissing, "PDF subset font is missing"))?;
    let (mut cursor_x, mut cursor_y) = (start_x, baseline);
    content
        .begin_text()
        .set_text_rendering_mode(rendering_mode)
        .set_font(Name(font.resource.as_bytes()), unit(size));
    for glyph in &run.glyphs {
        let gid = font.remapper.get(glyph.id).ok_or_else(|| {
            FileMakerError::new(ErrorCode::ExportWrite, "PDF subset omitted a used glyph")
        })?;
        let bytes = gid.to_be_bytes();
        content
            .set_text_matrix([
                1.0,
                0.0,
                0.0,
                1.0,
                cursor_x + unit(glyph.offset_x),
                cursor_y + unit(glyph.offset_y),
            ])
            .show(Str(&bytes));
        cursor_x += unit(glyph.advance_x);
        cursor_y += unit(glyph.advance_y);
    }
    content
        .set_text_rendering_mode(TextRenderingMode::Fill)
        .end_text();
    Ok(())
}

pub(super) fn render_flattened_run(
    content: &mut Content,
    run: &crate::GlyphRun,
    size: crate::Unit,
    start_x: f32,
    baseline: f32,
    context: &ExportContext<'_>,
) -> Result<()> {
    let font = context.fonts.get(&run.font)?;
    let face = FontRef::from_index(&font.bytes, font.face_index).map_err(|_| {
        FileMakerError::new(ErrorCode::FontMissing, "cannot parse PDF outline font")
    })?;
    let units_per_em = face
        .metrics(Size::unscaled(), LocationRef::default())
        .units_per_em;
    if units_per_em == 0 {
        return Err(FileMakerError::new(
            ErrorCode::FontMissing,
            "PDF outline font has no units-per-em",
        ));
    }
    let scale = unit(size) / f32::from(units_per_em);
    let outlines = face.outline_glyphs();
    let (mut cursor_x, mut cursor_y) = (start_x, baseline);
    for glyph in &run.glyphs {
        let mut outline = PdfOutline::new(
            content,
            cursor_x + unit(glyph.offset_x),
            cursor_y + unit(glyph.offset_y),
            scale,
        );
        if let Some(outline_glyph) = outlines.get(GlyphId::new(u32::from(glyph.id))) {
            outline_glyph
                .draw(
                    DrawSettings::unhinted(Size::unscaled(), LocationRef::default()),
                    &mut outline,
                )
                .map_err(|_| {
                    FileMakerError::new(ErrorCode::ExportWrite, "cannot draw PDF font outline")
                })?;
            outline.content.fill_nonzero();
        }
        cursor_x += unit(glyph.advance_x);
        cursor_y += unit(glyph.advance_y);
    }
    Ok(())
}

fn render_image(
    content: &mut Content,
    element: &ResolvedElement,
    page_height: f32,
    images: &BTreeMap<String, PdfImage>,
) {
    let Some(image) = images.get(element.id.as_str()) else {
        return;
    };
    let Some(placement) = element.image_placement else {
        return;
    };
    let destination = placement.destination;
    let width = unit(destination.size.width);
    let height = unit(destination.size.height);
    let x = unit(destination.origin.x);
    let y = page_height - unit(destination.origin.y) - height;
    let clip = placement.clip;
    content
        .save_state()
        .rect(
            unit(clip.origin.x),
            page_height - unit(clip.origin.y) - unit(clip.size.height),
            unit(clip.size.width),
            unit(clip.size.height),
        )
        .clip_nonzero()
        .end_path()
        .transform([width, 0.0, 0.0, height, x, y])
        .x_object(Name(image.resource.as_bytes()))
        .restore_state();
}
