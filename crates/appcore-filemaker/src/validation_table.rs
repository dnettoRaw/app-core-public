// =============================================================================
//        #######
//     ###       ###     F: validation_table.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded validation table contracts and behavior for this crate.

use crate::{
    ExportContext, ExportFormat, ExportRequest, PdfMode, PreflightOptions, ResolvedElement,
    ResolvedTableCell, Result, TextDiagnostic, ValidationCode, ValidationReport,
    ValidationSeverity,
};

pub(crate) fn inspect_layout(
    page: usize,
    element: &ResolvedElement,
    options: &PreflightOptions,
    report: &mut ValidationReport,
) -> Result<()> {
    let Some(table) = &element.table else {
        report.push(
            ValidationSeverity::Error,
            ValidationCode::Contract,
            Some(page),
            Some(element.id.as_str()),
            "table element has no resolved fragment",
            options.max_issues,
        );
        return Ok(());
    };
    let expected = table.columns.len();
    if expected == 0
        || !table.header.is_empty() && table.header.len() != expected
        || table.rows.iter().any(|row| row.cells.len() != expected)
        || !table.totals.is_empty() && table.totals.len() != expected
    {
        report.push(
            ValidationSeverity::Error,
            ValidationCode::Contract,
            Some(page),
            Some(element.id.as_str()),
            "resolved table rows do not match the column contract",
            options.max_issues,
        );
    }
    for cell in table
        .header
        .iter()
        .chain(table.rows.iter().flat_map(|row| &row.cells))
        .chain(&table.totals)
    {
        inspect_cell_layout(page, element, cell, options, report)?;
    }
    Ok(())
}

pub(crate) fn inspect_export(
    page: usize,
    element: &ResolvedElement,
    request: &ExportRequest,
    context: &ExportContext<'_>,
    options: &PreflightOptions,
    report: &mut ValidationReport,
) {
    let Some(table) = &element.table else {
        return;
    };
    for cell in table
        .header
        .iter()
        .chain(table.rows.iter().flat_map(|row| &row.cells))
        .chain(&table.totals)
    {
        inspect_cell_export(page, element, cell, request, context, options, report);
    }
}

fn inspect_cell_layout(
    page: usize,
    element: &ResolvedElement,
    cell: &ResolvedTableCell,
    options: &PreflightOptions,
    report: &mut ValidationReport,
) -> Result<()> {
    if !contains(element.bounds.layout, cell.bounds)? {
        report.push(
            ValidationSeverity::Error,
            ValidationCode::Overflow,
            Some(page),
            Some(element.id.as_str()),
            format!("table cell `{}` leaves its resolved fragment", cell.field),
            options.max_issues,
        );
    }
    for diagnostic in &cell.text_layout.diagnostics {
        let code = match diagnostic {
            TextDiagnostic::Clipped | TextDiagnostic::Ellipsized => ValidationCode::Overflow,
            TextDiagnostic::ColorEmojiRequiresExporter
            | TextDiagnostic::VerticalWritingUnavailable => ValidationCode::Capability,
            TextDiagnostic::Shrunk => continue,
        };
        report.push(
            ValidationSeverity::Warning,
            code,
            Some(page),
            Some(element.id.as_str()),
            format!("table cell `{}` diagnostic: {diagnostic:?}", cell.field),
            options.max_issues,
        );
    }
    Ok(())
}

fn inspect_cell_export(
    page: usize,
    element: &ResolvedElement,
    cell: &ResolvedTableCell,
    request: &ExportRequest,
    context: &ExportContext<'_>,
    options: &PreflightOptions,
    report: &mut ValidationReport,
) {
    if request.format == ExportFormat::Pdf
        && matches!(request.pdf_mode, PdfMode::Editable | PdfMode::Hybrid)
    {
        for run in cell.text_layout.lines.iter().flat_map(|line| &line.runs) {
            if context.fonts.get(&run.font).is_err() {
                report.push(
                    ValidationSeverity::Error,
                    ValidationCode::FontEmbedding,
                    Some(page),
                    Some(element.id.as_str()),
                    format!("table font `{}` is unavailable for embedding", run.font),
                    options.max_issues,
                );
            }
        }
    }
}

fn contains(outer: crate::Rect, inner: crate::Rect) -> Result<bool> {
    Ok(inner.origin.x >= outer.origin.x
        && inner.origin.y >= outer.origin.y
        && inner.right()? <= outer.right()?
        && inner.bottom()? <= outer.bottom()?)
}
