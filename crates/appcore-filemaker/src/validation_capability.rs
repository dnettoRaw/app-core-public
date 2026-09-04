// =============================================================================
//        #######
//     ###       ###     F: validation_capability.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use crate::{
    Color, ComputedStyle, ExportFormat, ExportRequest, PreflightOptions, ResolvedElement,
    ValidationCode, ValidationReport, ValidationSeverity,
};

pub(crate) fn inspect_paint(
    page: usize,
    element: &ResolvedElement,
    request: &ExportRequest,
    options: &PreflightOptions,
    report: &mut ValidationReport,
) {
    if request.format != ExportFormat::Pdf && styles(element).any(|style| has_cmyk(style, request))
    {
        report.push(
            ValidationSeverity::Warning,
            ValidationCode::Capability,
            Some(page),
            Some(element.id.as_str()),
            "requested exporter converts CMYK paint to RGB",
            options.max_issues,
        );
    }
    if request.format == ExportFormat::Jpeg
        && styles(element).any(|style| has_transparency(style, request))
    {
        report.push(
            ValidationSeverity::Warning,
            ValidationCode::Capability,
            Some(page),
            Some(element.id.as_str()),
            "JPEG flattens transparent paint on white",
            options.max_issues,
        );
    }
}

pub(crate) fn raster_asset_has_alpha(
    asset: &crate::Asset,
    placement: crate::ImagePlacement,
) -> crate::Result<bool> {
    let decoded = image::load_from_memory(&asset.bytes).map_err(|error| {
        crate::FileMakerError::new(
            crate::ErrorCode::AssetInvalid,
            format!("cannot decode `{}` during preflight: {error}", asset.name),
        )
    })?;
    let cropped = decoded
        .crop_imm(
            placement.source.x,
            placement.source.y,
            placement.source.width,
            placement.source.height,
        )
        .to_rgba8();
    Ok(cropped.pixels().any(|pixel| pixel[3] < 255))
}

fn styles(element: &ResolvedElement) -> impl Iterator<Item = &ComputedStyle> {
    std::iter::once(&element.style).chain(element.table.iter().flat_map(|table| {
        table
            .header
            .iter()
            .chain(table.rows.iter().flat_map(|row| &row.cells))
            .chain(&table.totals)
            .map(|cell| &cell.style)
    }))
}

fn has_cmyk(style: &ComputedStyle, request: &ExportRequest) -> bool {
    selected_colors(style, request).any(|color| matches!(color, Color::Cmyk { .. }))
}

fn has_transparency(style: &ComputedStyle, request: &ExportRequest) -> bool {
    request
        .style_override
        .and_then(|value| value.opacity)
        .unwrap_or(style.opacity)
        < 1_000_000
        || selected_colors(style, request)
            .any(|color| matches!(color, Color::Rgba { a, .. } if a < 255))
}

fn selected_colors(style: &ComputedStyle, request: &ExportRequest) -> impl Iterator<Item = Color> {
    let override_style = request.style_override.unwrap_or_default();
    [
        override_style.fill.or(style.fill),
        override_style.stroke.or(style.stroke),
        Some(override_style.color.unwrap_or(style.color)),
    ]
    .into_iter()
    .flatten()
}
