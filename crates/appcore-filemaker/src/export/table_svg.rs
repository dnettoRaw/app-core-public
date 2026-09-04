// =============================================================================
//        #######
//     ###       ###     F: table_svg.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded table svg contracts and behavior for this crate.

use super::bounded_string::{output_limit_error, FormattedOutput};
use super::markup::{color, escape, opacity, points};
use crate::{
    Color, ErrorCode, ExportLossKind, ExportLossReport, FileMakerError, ResolvedElement,
    ResolvedTableCell, Result,
};

pub(super) fn render(svg: &mut dyn FormattedOutput, element: &ResolvedElement) -> Result<()> {
    let table = element.table.as_ref().ok_or_else(|| {
        FileMakerError::new(
            ErrorCode::ExportWrite,
            "resolved SVG table has no table fragment",
        )
    })?;
    write!(
        svg,
        "<g id=\"{}\" data-table-fragment=\"{}\">",
        escape(element.id.as_str()),
        table.index
    )
    .map_err(svg_error)?;
    for cell in table
        .header
        .iter()
        .chain(table.rows.iter().flat_map(|row| &row.cells))
        .chain(&table.totals)
    {
        render_cell(svg, cell)?;
    }
    svg.push_str("</g>")?;
    Ok(())
}

fn render_cell(svg: &mut dyn FormattedOutput, cell: &ResolvedTableCell) -> Result<()> {
    write!(
        svg,
        "<rect data-field=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\" opacity=\"{}\"/>",
        escape(&cell.field),
        points(cell.bounds.origin.x),
        points(cell.bounds.origin.y),
        points(cell.bounds.size.width),
        points(cell.bounds.size.height),
        cell.style.fill.map_or_else(|| "none".to_owned(), color),
        cell.style.stroke.map_or_else(|| "none".to_owned(), color),
        points(cell.style.stroke_width),
        opacity(cell.style.opacity),
    )
    .map_err(svg_error)?;
    let vertical = cell.text_layout.writing_mode == crate::WritingMode::Vertical;
    let (mut line_x, mut line_y) = if vertical {
        (
            cell.bounds.origin.x.checked_add(cell.bounds.size.width)?,
            cell.bounds.origin.y,
        )
    } else {
        (
            cell.bounds.origin.x,
            cell.bounds
                .origin
                .y
                .checked_add(cell.text_layout.font_size)?,
        )
    };
    write!(
        svg,
        "<text data-cell-text=\"{}\" font-size=\"{}\" fill=\"{}\" opacity=\"{}\"{}>",
        escape(&cell.field),
        points(cell.text_layout.font_size),
        color(cell.style.color),
        opacity(cell.style.opacity),
        if vertical {
            " writing-mode=\"vertical-rl\""
        } else {
            ""
        },
    )
    .map_err(svg_error)?;
    for line in &cell.text_layout.lines {
        write!(
            svg,
            "<tspan x=\"{}\" y=\"{}\">",
            points(line_x),
            points(line_y)
        )
        .map_err(svg_error)?;
        for run in &line.runs {
            write!(
                svg,
                "<tspan font-family=\"{}\" direction=\"{}\">{}</tspan>",
                escape(&run.font),
                if run.rtl { "rtl" } else { "ltr" },
                escape(&run.text)
            )
            .map_err(svg_error)?;
        }
        svg.push_str("</tspan>")?;
        if vertical {
            line_x = line_x.checked_sub(line.height)?;
        } else {
            line_y = line_y.checked_add(line.height)?;
        }
    }
    svg.push_str("</text>")?;
    Ok(())
}

pub(super) fn record_losses(element: &ResolvedElement, losses: &mut ExportLossReport) {
    let Some(table) = &element.table else {
        return;
    };
    for cell in table
        .header
        .iter()
        .chain(table.rows.iter().flat_map(|row| &row.cells))
        .chain(&table.totals)
    {
        record_cmyk(cell, element, losses);
    }
}

fn record_cmyk(cell: &ResolvedTableCell, element: &ResolvedElement, losses: &mut ExportLossReport) {
    if [cell.style.fill, cell.style.stroke, Some(cell.style.color)]
        .into_iter()
        .flatten()
        .any(|value| matches!(value, Color::Cmyk { .. }))
    {
        losses.push(
            ExportLossKind::CmykConvertedToRgb,
            Some(element.id.as_str()),
            "SVG table cell has no native CMYK paint",
        );
    }
}

fn svg_error(_: std::fmt::Error) -> FileMakerError {
    output_limit_error()
}
