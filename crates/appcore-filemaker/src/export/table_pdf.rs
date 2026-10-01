// =============================================================================
//        #######
//     ###       ###     F: table_pdf.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded table pdf contracts and behavior for this crate.

use std::collections::BTreeMap;

use pdf_writer::Content;

use super::pdf_font::{unit, PdfFont};
use super::pdf_paint::{apply_opacity, effective_opacity, set_fill, set_stroke};
use super::pdf_render::render_text_layout;
use crate::{
    ErrorCode, ExportContext, FileMakerError, PdfMode, ResolvedElement, ResolvedTableCell, Result,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn render(
    content: &mut Content,
    element: &ResolvedElement,
    page_height: f32,
    mode: PdfMode,
    context: &ExportContext<'_>,
    fonts: &BTreeMap<String, PdfFont>,
    opacities: &BTreeMap<u32, pdf_writer::Ref>,
) -> Result<()> {
    let table = element.table.as_ref().ok_or_else(|| {
        FileMakerError::new(
            ErrorCode::ExportWrite,
            "resolved PDF table has no table fragment",
        )
    })?;
    for cell in table
        .header
        .iter()
        .chain(table.rows.iter().flat_map(|row| &row.cells))
        .chain(&table.totals)
    {
        render_cell(content, cell, page_height, mode, context, fonts, opacities)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn render_cell(
    content: &mut Content,
    cell: &ResolvedTableCell,
    page_height: f32,
    mode: PdfMode,
    context: &ExportContext<'_>,
    fonts: &BTreeMap<String, PdfFont>,
    opacities: &BTreeMap<u32, pdf_writer::Ref>,
) -> Result<()> {
    let x = unit(cell.bounds.origin.x);
    let y = page_height - unit(cell.bounds.origin.y) - unit(cell.bounds.size.height);
    let width = unit(cell.bounds.size.width);
    let height = unit(cell.bounds.size.height);
    if let Some(fill) = cell.style.fill {
        apply_opacity(
            content,
            effective_opacity(cell.style.opacity, fill),
            opacities,
        );
        set_fill(content, fill);
        content.rect(x, y, width, height).fill_nonzero();
    }
    if let Some(stroke) = cell.style.stroke {
        apply_opacity(
            content,
            effective_opacity(cell.style.opacity, stroke),
            opacities,
        );
        set_stroke(content, stroke);
        content.set_line_width(unit(cell.style.stroke_width));
        let sides = cell.style.stroke_sides;
        for (enabled, (x1, y1), (x2, y2)) in [
            (sides.top, (x, y + height), (x + width, y + height)),
            (sides.right, (x + width, y), (x + width, y + height)),
            (sides.bottom, (x, y), (x + width, y)),
            (sides.left, (x, y), (x, y + height)),
        ] {
            if enabled {
                content.move_to(x1, y1).line_to(x2, y2).stroke();
            }
        }
    }
    let text_bounds = cell.content_bounds()?;
    content
        .save_state()
        .rect(
            unit(text_bounds.origin.x),
            page_height - unit(text_bounds.origin.y) - unit(text_bounds.size.height),
            unit(text_bounds.size.width),
            unit(text_bounds.size.height),
        )
        .clip_nonzero()
        .end_path();
    apply_opacity(
        content,
        effective_opacity(cell.style.opacity, cell.style.color),
        opacities,
    );
    set_fill(content, cell.style.color);
    render_text_layout(
        content,
        &cell.text_layout,
        text_bounds,
        page_height,
        mode,
        context,
        fonts,
        &cell.style,
    )?;
    content.restore_state();
    Ok(())
}
