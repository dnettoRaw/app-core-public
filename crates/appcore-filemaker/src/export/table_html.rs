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
        let writing_mode = if cell.text_layout.writing_mode == crate::WritingMode::Vertical {
            "writing-mode:vertical-rl;"
        } else {
            ""
        };
        write!(
            html,
            "<{tag} data-field=\"{}\" style=\"width:{}pt;height:{}pt;{writing_mode}background:{};border:{}pt solid {};color:{};font-size:{}pt;opacity:{};overflow:hidden\">",
            escape(&cell.field),
            points(cell.bounds.size.width),
            points(cell.bounds.size.height),
            cell.style.fill.map_or_else(|| "transparent".to_owned(), color),
            points(cell.style.stroke_width),
            cell.style.stroke.map_or_else(|| "transparent".to_owned(), color),
            color(cell.style.color),
            points(cell.text_layout.font_size),
            opacity(cell.style.opacity),
        )
        .map_err(html_error)?;
        for (line_index, line) in cell.text_layout.lines.iter().enumerate() {
            if line_index > 0 {
                html.push_str("<br>")?;
            }
            for run in &line.runs {
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
        write!(html, "</{tag}>").map_err(html_error)?;
    }
    Ok(())
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
