// =============================================================================
//        #######
//     ###       ###     F: table_html.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded table html contracts and behavior for this crate.

use super::bounded_string::{output_limit_error, FormattedOutput};
use super::markup::{color, escape, opacity, points};
use crate::{
    Color, ErrorCode, ExportLossKind, ExportLossReport, FileMakerError, HtmlMode, ResolvedElement,
    ResolvedTableCell, Result,
};

pub(super) fn render(
    html: &mut dyn FormattedOutput,
    element: &ResolvedElement,
    mode: HtmlMode,
    attributes: &str,
    geometry_style: &str,
) -> Result<()> {
    let table = element.table.as_ref().ok_or_else(|| {
        FileMakerError::new(
            ErrorCode::ExportWrite,
            "resolved HTML table has no table fragment",
        )
    })?;
    write!(
        html,
        "<table id=\"{}\" data-table-fragment=\"{}\" {attributes} style=\"{geometry_style}border-collapse:collapse;table-layout:fixed\">",
        escape(element.id.as_str()),
        table.index,
    )
    .map_err(html_error)?;
    if !table.header.is_empty() {
        html.push_str("<thead><tr>")?;
        render_cells(html, &table.header, "th")?;
        html.push_str("</tr></thead>")?;
    }
    html.push_str("<tbody>")?;
    for row in &table.rows {
        write!(
            html,
            "<tr data-source-row=\"{}\"{}>",
            row.source_index,
            row.group_start.as_ref().map_or_else(String::new, |group| {
                format!(" data-group-start=\"{}\"", escape(group))
            })
        )
        .map_err(html_error)?;
        render_cells(html, &row.cells, "td")?;
        html.push_str("</tr>")?;
    }
    html.push_str("</tbody>")?;
    if !table.totals.is_empty() {
        html.push_str("<tfoot><tr>")?;
        render_cells(html, &table.totals, "td")?;
        html.push_str("</tr></tfoot>")?;
    }
    html.push_str("</table>")?;
    if mode == HtmlMode::Semantic && table.index > 0 {
        write!(
            html,
            "<!-- continuation of table {} -->",
            escape(element.id.as_str())
        )
        .map_err(html_error)?;
    }
    Ok(())
}

fn render_cells(
    html: &mut dyn FormattedOutput,
    cells: &[ResolvedTableCell],
    tag: &str,
) -> Result<()> {
    for cell in cells {
        let text_bounds = cell.content_bounds()?;
        let padding_top = text_bounds.origin.y.checked_sub(cell.bounds.origin.y)?;
        let padding_right = cell
            .bounds
            .origin
            .x
            .checked_add(cell.bounds.size.width)?
            .checked_sub(text_bounds.origin.x.checked_add(text_bounds.size.width)?)?
            .checked_add(cell.text_layout.padding_inline)?;
        let padding_bottom = cell
            .bounds
            .origin
            .y
            .checked_add(cell.bounds.size.height)?
            .checked_sub(text_bounds.origin.y.checked_add(text_bounds.size.height)?)?;
        let padding_left = text_bounds
            .origin
            .x
            .checked_sub(cell.bounds.origin.x)?
            .checked_add(cell.text_layout.padding_inline)?;
        let text_align = match cell.text_layout.align_x {
            crate::Alignment::Start => "text-align:start;",
            crate::Alignment::Center => "text-align:center;",
            crate::Alignment::End => "text-align:end;",
        };
        let writing_mode = if cell.text_layout.writing_mode == crate::WritingMode::Vertical {
            "writing-mode:vertical-rl;"
        } else {
            ""
        };
        let text_decoration = if cell.style.underline
            && cell.text_layout.writing_mode == crate::WritingMode::Horizontal
        {
            "text-decoration:underline;"
        } else {
            ""
        };
        write!(
            html,
            "<{tag} data-field=\"{}\" style=\"box-sizing:border-box;vertical-align:top;width:{}pt;height:{}pt;{writing_mode}{text_align}{text_decoration}padding:{}pt {}pt {}pt {}pt;background:{};border-top:{};border-right:{};border-bottom:{};border-left:{};color:{};font-size:{}pt;opacity:{};overflow:hidden\">",
            escape(&cell.field),
            points(cell.bounds.size.width),
            points(cell.bounds.size.height),
            points(padding_top),
            points(padding_right),
            points(padding_bottom),
            points(padding_left),
            cell.style.fill.map_or_else(|| "transparent".to_owned(), color),
            html_border(cell, cell.style.stroke_sides.top),
            html_border(cell, cell.style.stroke_sides.right),
            html_border(cell, cell.style.stroke_sides.bottom),
            html_border(cell, cell.style.stroke_sides.left),
            color(cell.style.color),
            points(cell.text_layout.font_size),
            opacity(cell.style.opacity),
        )
        .map_err(html_error)?;
        let paint_offset = if cell.text_layout.paint_offset_y == crate::Unit::ZERO {
            String::new()
        } else {
            format!(
                "<span style=\"position:relative;top:{}pt\">",
                points(cell.text_layout.paint_offset_y)
            )
        };
        html.push_str(&paint_offset)?;
        for (line_index, line) in cell.text_layout.lines.iter().enumerate() {
            if line_index > 0 {
                html.push_str("<br>")?;
            }
            for run in &line.runs {
                if run.text.is_empty() && run.glyphs.is_empty() && run.width > crate::Unit::ZERO {
                    write!(
                        html,
                        "<span aria-hidden=\"true\" style=\"display:inline-block;width:{}pt\"></span>",
                        points(run.width)
                    )
                    .map_err(html_error)?;
                    continue;
                }
                write!(
                    html,
                    "<span style=\"font-family:'{}';direction:{}\">{}</span>",
                    escape(&run.font),
                    if run.rtl { "rtl" } else { "ltr" },
                    escape(&run.text)
                )
                .map_err(html_error)?;
            }
        }
        if !paint_offset.is_empty() {
            html.push_str("</span>")?;
        }
        write!(html, "</{tag}>").map_err(html_error)?;
    }
    Ok(())
}

fn html_border(cell: &crate::ResolvedTableCell, enabled: bool) -> String {
    if enabled {
        cell.style.stroke.map_or_else(
            || "none".to_owned(),
            |stroke| {
                format!(
                    "{}pt solid {}",
                    points(cell.style.stroke_width),
                    color(stroke)
                )
            },
        )
    } else {
        "none".to_owned()
    }
}

pub(super) fn record_losses(element: &ResolvedElement, losses: &mut ExportLossReport) {
    let Some(table) = &element.table else {
        return;
    };
    if table
        .header
        .iter()
        .chain(table.rows.iter().flat_map(|row| &row.cells))
        .chain(&table.totals)
        .any(|cell| {
            [cell.style.fill, cell.style.stroke, Some(cell.style.color)]
                .into_iter()
                .flatten()
                .any(|paint| matches!(paint, Color::Cmyk { .. }))
        })
    {
        losses.push(
            ExportLossKind::CmykConvertedToRgb,
            Some(element.id.as_str()),
            "HTML table cells have no portable CMYK paint",
        );
    }
}

fn html_error(_: std::fmt::Error) -> FileMakerError {
    output_limit_error()
}
