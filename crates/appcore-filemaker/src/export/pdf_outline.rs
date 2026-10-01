// =============================================================================
//        #######
//     ###       ###     F: pdf_outline.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Converts shaped glyph outlines to bounded PDF paths when text needs outlines.
//!
//! The routine uses the glyph identity and font instance already selected by
//! text shaping; it never substitutes a host font or changes layout metrics.

use pdf_writer::Content;
use skrifa::{
    instance::{LocationRef, Size},
    outline::{DrawSettings, OutlinePen},
    FontRef, GlyphId, MetadataProvider,
};

use super::pdf_font::unit;
use crate::{ErrorCode, ExportContext, FileMakerError, GlyphRun, Result, Unit};

pub(super) fn render_flattened_run(
    content: &mut Content,
    run: &GlyphRun,
    size: Unit,
    start_x: f32,
    baseline: f32,
    context: &ExportContext<'_>,
) -> Result<()> {
    if context.fonts.standard_face(&run.font).is_some() {
        return Err(FileMakerError::new(
            ErrorCode::ExportUnsupported,
            "PDF Standard fonts have no explicit outlines for flattened export",
        ));
    }
    let font = context.fonts.get_outline_asset(&run.font)?;
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

pub(super) struct PdfOutline<'a> {
    pub(super) content: &'a mut Content,
    origin_x: f32,
    origin_y: f32,
    scale: f32,
    current: (f32, f32),
}

impl<'a> PdfOutline<'a> {
    pub(super) fn new(content: &'a mut Content, origin_x: f32, origin_y: f32, scale: f32) -> Self {
        Self {
            content,
            origin_x,
            origin_y,
            scale,
            current: (0.0, 0.0),
        }
    }

    fn point(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.origin_x + x * self.scale,
            self.origin_y + y * self.scale,
        )
    }
}

impl OutlinePen for PdfOutline<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.current = (x, y);
        let point = self.point(x, y);
        self.content.move_to(point.0, point.1);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.current = (x, y);
        let point = self.point(x, y);
        self.content.line_to(point.0, point.1);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (x0, y0) = self.current;
        let c1 = self.point(x0 + (x1 - x0) * 2.0 / 3.0, y0 + (y1 - y0) * 2.0 / 3.0);
        let c2 = self.point(x + (x1 - x) * 2.0 / 3.0, y + (y1 - y) * 2.0 / 3.0);
        let end = self.point(x, y);
        self.content.cubic_to(c1.0, c1.1, c2.0, c2.1, end.0, end.1);
        self.current = (x, y);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let c1 = self.point(x1, y1);
        let c2 = self.point(x2, y2);
        let end = self.point(x, y);
        self.content.cubic_to(c1.0, c1.1, c2.0, c2.1, end.0, end.1);
        self.current = (x, y);
    }

    fn close(&mut self) {
        self.content.close_path();
    }
}
