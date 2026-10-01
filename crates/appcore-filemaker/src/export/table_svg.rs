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
    for (cell_index, cell) in table
        .header
        .iter()
        .chain(table.rows.iter().flat_map(|row| &row.cells))
        .chain(&table.totals)
        .enumerate()
    {
        render_cell(svg, cell, element.id.as_str(), table.index, cell_index)?;
    }
    svg.push_str("</g>")?;
    Ok(())
}

fn render_cell(
    svg: &mut dyn FormattedOutput,
    cell: &ResolvedTableCell,
    table_id: &str,
    fragment_index: usize,
    cell_index: usize,
) -> Result<()> {
    render_cell_background(svg, cell)?;
    render_cell_text(svg, cell, table_id, fragment_index, cell_index)
}

fn render_cell_background(svg: &mut dyn FormattedOutput, cell: &ResolvedTableCell) -> Result<()> {
    write!(
        svg,
        "<rect data-field=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\" stroke=\"none\" opacity=\"{}\"/>",
        escape(&cell.field),
        points(cell.bounds.origin.x),
        points(cell.bounds.origin.y),
        points(cell.bounds.size.width),
        points(cell.bounds.size.height),
        cell.style.fill.map_or_else(|| "none".to_owned(), color),
        opacity(cell.style.opacity),
    )
    .map_err(svg_error)?;
    if let Some(stroke) = cell.style.stroke {
        let x = cell.bounds.origin.x;
        let y = cell.bounds.origin.y;
        let right = x.checked_add(cell.bounds.size.width)?;
        let bottom = y.checked_add(cell.bounds.size.height)?;
        let sides = cell.style.stroke_sides;
        for (enabled, x1, y1, x2, y2) in [
            (sides.top, x, y, right, y),
            (sides.right, right, y, right, bottom),
            (sides.bottom, x, bottom, right, bottom),
            (sides.left, x, y, x, bottom),
        ] {
            if enabled {
                write!(svg, "<path d=\"M {} {} L {} {}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" opacity=\"{}\"/>", points(x1), points(y1), points(x2), points(y2), color(stroke), points(cell.style.stroke_width), opacity(cell.style.opacity)).map_err(svg_error)?;
            }
        }
    }
    Ok(())
}

fn render_cell_text(
    svg: &mut dyn FormattedOutput,
    cell: &ResolvedTableCell,
    table_id: &str,
    fragment_index: usize,
    cell_index: usize,
) -> Result<()> {
    let text_bounds = cell.content_bounds()?;
    let shifted = cell.text_layout.paint_offset_y != crate::Unit::ZERO;
    let clip_id = format!("{table_id}-fragment-{fragment_index}-cell-{cell_index}");
    if shifted {
        write!(
            svg,
            "<defs><clipPath id=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath></defs>",
            escape(&clip_id),
            points(text_bounds.origin.x),
            points(text_bounds.origin.y),
            points(text_bounds.size.width),
            points(text_bounds.size.height),
        )
        .map_err(svg_error)?;
    }
    let vertical = cell.text_layout.writing_mode == crate::WritingMode::Vertical;
    let (mut line_x, mut line_y) = if vertical {
        (
            text_bounds.origin.x.checked_add(text_bounds.size.width)?,
            text_bounds
                .origin
                .y
                .checked_add(cell.text_layout.paint_offset_y)?,
        )
    } else {
        (
            text_bounds.origin.x,
            text_bounds
                .origin
                .y
                .checked_add(cell.text_layout.font_size)?
                .checked_add(cell.text_layout.paint_offset_y)?,
        )
    };
    write!(
        svg,
        "<text data-cell-text=\"{}\" font-size=\"{}\" fill=\"{}\" opacity=\"{}\"{}{}{}>",
        escape(&cell.field),
        points(cell.text_layout.font_size),
        color(cell.style.color),
        opacity(cell.style.opacity),
        if cell.style.underline && !vertical {
            " text-decoration=\"underline\""
        } else {
            ""
        },
        if vertical {
            " writing-mode=\"vertical-rl\""
        } else {
            ""
        },
        if shifted {
            format!(" clip-path=\"url(#{})\"", escape(&clip_id))
        } else {
            String::new()
        },
    )
    .map_err(svg_error)?;
    for line in &cell.text_layout.lines {
        let inline_offset = cell.text_layout.inline_offset(
            if vertical {
                text_bounds.size.height
            } else {
                text_bounds.size.width
            },
            line,
        )?;
        write!(
            svg,
            "<tspan x=\"{}\" y=\"{}\">",
            points(if vertical {
                line_x
            } else {
                line_x.checked_add(inline_offset)?
            }),
            points(if vertical {
                line_y.checked_add(inline_offset)?
            } else {
                line_y
            })
        )
        .map_err(svg_error)?;
        for run in &line.runs {
            if run.text.is_empty() && run.glyphs.is_empty() && run.width > crate::Unit::ZERO {
                write!(svg, "<tspan dx=\"{}\"></tspan>", points(run.width)).map_err(svg_error)?;
                continue;
            }
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
