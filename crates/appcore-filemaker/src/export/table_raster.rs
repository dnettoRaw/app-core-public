// =============================================================================
//        #######
//     ###       ###     F: table_raster.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use tiny_skia::{FillRule, PathBuilder, Pixmap, Stroke};

use super::raster::{paint, raster_transform, to_pixel};
use crate::{ErrorCode, ExportContext, FileMakerError, ResolvedElement, ResolvedTableCell, Result};

pub(super) fn render(
    pixmap: &mut Pixmap,
    element: &ResolvedElement,
    context: &ExportContext<'_>,
    scale: f32,
    page_y: f32,
) -> Result<()> {
    let table = element.table.as_ref().ok_or_else(|| {
        FileMakerError::new(
            ErrorCode::ExportWrite,
            "resolved raster table has no table fragment",
        )
    })?;
    let transform = raster_transform(element.transform, scale, page_y);
    for cell in table
        .header
        .iter()
        .chain(table.rows.iter().flat_map(|row| &row.cells))
        .chain(&table.totals)
    {
        render_cell(pixmap, cell, context, scale, page_y, transform)?;
    }
    Ok(())
}

fn render_cell(
    pixmap: &mut Pixmap,
    cell: &ResolvedTableCell,
    context: &ExportContext<'_>,
    scale: f32,
    page_y: f32,
    transform: tiny_skia::Transform,
) -> Result<()> {
    let rect = tiny_skia::Rect::from_xywh(
        to_pixel(cell.bounds.origin.x, scale),
        to_pixel(cell.bounds.origin.y, scale) + page_y,
        to_pixel(cell.bounds.size.width, scale).max(0.001),
        to_pixel(cell.bounds.size.height, scale).max(0.001),
    )
    .ok_or_else(|| table_raster_error("invalid raster table cell"))?;
    let mut builder = PathBuilder::new();
    builder.push_rect(rect);
    let path = builder
        .finish()
        .ok_or_else(|| table_raster_error("cannot build raster table cell"))?;
    if let Some(fill) = cell.style.fill {
        pixmap.fill_path(
            &path,
            &paint(fill, cell.style.opacity),
            FillRule::Winding,
            transform,
            None,
        );
    }
    if let Some(stroke_color) = cell.style.stroke {
        pixmap.stroke_path(
            &path,
            &paint(stroke_color, cell.style.opacity),
            &Stroke {
                width: to_pixel(cell.style.stroke_width, scale),
                ..Stroke::default()
            },
            transform,
            None,
        );
    }
    super::raster_text::render_layout(
        pixmap,
        &cell.text_layout,
        cell.bounds,
        &cell.style,
        context,
        scale,
        page_y,
        transform,
        Some(cell.bounds),
    )
}

fn table_raster_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::ExportUnsupported, message)
}
