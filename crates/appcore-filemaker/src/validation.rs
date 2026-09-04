// =============================================================================
//        #######
//     ###       ###     F: validation.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded validation contracts and behavior for this crate.

use serde::{Deserialize, Serialize};

use crate::{
    DataValue, ElementKind, ErrorCode, ExportContext, ExportFormat, ExportRequest, FileMakerError,
    HtmlMode, OperationControl, PdfMode, ResolvedScene, ResourceLimits, Result, TemplateIr,
};

/// Severity retained in machine-readable validation output.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationSeverity {
    /// Rendering can proceed unless strict warnings are requested.
    Warning,
    /// Rendering must not proceed.
    Error,
}

/// Stable validation/preflight condition.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationCode {
    /// Binding/condition/repeat expression is invalid.
    Binding,
    /// Typed input does not satisfy its structural or declared data contract.
    Data,
    /// Referenced asset is absent or invalid.
    Asset,
    /// Font/glyph layout cannot be preserved.
    Glyph,
    /// Resolved elements overlap.
    Collision,
    /// Visual bounds leave the page.
    Overflow,
    /// Effective raster resolution is below policy.
    Dpi,
    /// Requested exporter cannot preserve an element.
    Capability,
    /// Editable PDF font embedding is not configured.
    FontEmbedding,
    /// Required accessibility metadata is unavailable.
    Accessibility,
    /// A generic schema/data/layout invariant failed.
    Contract,
    /// Diagnostic/comparison budget was exhausted.
    Budget,
}

/// One bounded first-class validation issue.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ValidationIssue {
    /// Severity.
    pub severity: ValidationSeverity,
    /// Stable condition.
    pub code: ValidationCode,
    /// Optional page.
    pub page: Option<usize>,
    /// Optional element.
    pub element: Option<String>,
    /// Bounded explanation.
    pub message: String,
}

/// Deterministic validation output.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ValidationReport {
    /// Issues in discovery order.
    pub issues: Vec<ValidationIssue>,
    /// Whether additional issues were omitted by the caller's bound.
    #[serde(default)]
    pub truncated: bool,
}

impl ValidationReport {
    /// Returns true when at least one hard error exists.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.issues
            .iter()
            .any(|issue| issue.severity == ValidationSeverity::Error)
    }

    /// Returns true when at least one warning exists.
    #[must_use]
    pub fn has_warnings(&self) -> bool {
        self.issues
            .iter()
            .any(|issue| issue.severity == ValidationSeverity::Warning)
    }

    /// Enforces errors, plus warnings when strict mode is active.
    pub fn enforce(&self, strict: bool) -> Result<()> {
        if self.truncated {
            return Err(FileMakerError::new(
                ErrorCode::Validation,
                "validation report was truncated before completion",
            ));
        }
        let rejected = self.issues.iter().find(|issue| {
            issue.severity == ValidationSeverity::Error
                || (strict && issue.severity == ValidationSeverity::Warning)
        });
        if let Some(issue) = rejected {
            return Err(FileMakerError::new(
                ErrorCode::Validation,
                format!("validation rejected {:?}: {}", issue.code, issue.message),
            ));
        }
        Ok(())
    }

    pub(crate) fn push(
        &mut self,
        severity: ValidationSeverity,
        code: ValidationCode,
        page: Option<usize>,
        element: Option<&str>,
        message: impl Into<String>,
        max_issues: usize,
    ) {
        if self.issues.len() >= max_issues {
            self.truncated = true;
            return;
        }
        let mut message = message.into();
        message.truncate(512);
        self.issues.push(ValidationIssue {
            severity,
            code,
            page,
            element: element.map(ToOwned::to_owned),
            message,
        });
    }
}

/// Caller-selected preflight policy.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PreflightOptions {
    /// Treat warnings as rejection.
    pub strict: bool,
    /// Require accessibility metadata from the current schema.
    pub require_accessibility: bool,
    /// Minimum effective image resolution.
    pub minimum_image_dpi: u32,
    /// Maximum retained diagnostics.
    pub max_issues: usize,
}

impl Default for PreflightOptions {
    fn default() -> Self {
        Self {
            strict: false,
            require_accessibility: false,
            minimum_image_dpi: 150,
            max_issues: 1_000,
        }
    }
}

/// Validates reusable template IR and binding syntax.
pub fn validate_template(template: &TemplateIr, limits: &ResourceLimits) -> ValidationReport {
    let mut report = ValidationReport::default();
    crate::validation_data::inspect_template(template, limits, &mut report);
    report
}

/// Validates bounded typed data, its optional schema, and every binding.
pub fn validate_data(
    template: &TemplateIr,
    data: &DataValue,
    limits: &ResourceLimits,
) -> ValidationReport {
    let mut report = ValidationReport::default();
    crate::validation_data::inspect_data(template, data, limits, &mut report);
    report
}

/// Validates resolved page, glyph, table, overflow, and collision invariants.
pub fn validate_layout(
    scene: &ResolvedScene,
    limits: &ResourceLimits,
    max_issues: usize,
    control: &OperationControl,
) -> Result<ValidationReport> {
    if max_issues == 0 {
        return Err(FileMakerError::new(
            ErrorCode::Validation,
            "layout validation requires a non-zero issue bound",
        ));
    }
    crate::validation_layout::inspect(
        scene,
        limits,
        &PreflightOptions {
            max_issues,
            ..PreflightOptions::default()
        },
        control,
    )
}

/// Runs exporter-aware validation over a fully resolved scene.
pub fn preflight(
    scene: &ResolvedScene,
    request: &ExportRequest,
    context: &ExportContext<'_>,
    options: &PreflightOptions,
    control: &OperationControl,
) -> Result<ValidationReport> {
    if options.max_issues == 0 || options.minimum_image_dpi == 0 {
        return Err(FileMakerError::new(
            ErrorCode::Validation,
            "preflight options require non-zero issue and DPI bounds",
        ));
    }
    crate::export::validate_request(scene, request, context.limits)?;
    let mut report = crate::validation_layout::inspect(scene, context.limits, options, control)?;
    inspect_accessibility(request, options, &mut report);
    for page in &scene.pages {
        for element in &page.elements {
            inspect_element(page.index, element, request, context, options, &mut report)?;
        }
    }
    report.enforce(options.strict)?;
    Ok(report)
}

fn inspect_element(
    page: usize,
    element: &crate::ResolvedElement,
    request: &ExportRequest,
    context: &ExportContext<'_>,
    options: &PreflightOptions,
    report: &mut ValidationReport,
) -> Result<()> {
    crate::validation_capability::inspect_paint(page, element, request, options, report);
    if matches!(
        element.kind,
        ElementKind::Chart | ElementKind::Qr | ElementKind::Barcode
    ) {
        report.push(
            ValidationSeverity::Error,
            ValidationCode::Capability,
            Some(page),
            Some(element.id.as_str()),
            "requested exporter has no renderer for this prepared element kind",
            options.max_issues,
        );
    }
    if element.kind == ElementKind::Text {
        inspect_text(page, element, request, context, options, report);
    }
    if element.kind == ElementKind::Image {
        inspect_image(page, element, request, context, options, report)?;
    }
    if element.kind == ElementKind::Table {
        crate::validation_table::inspect_export(page, element, request, context, options, report);
    }
    Ok(())
}

fn inspect_text(
    page: usize,
    element: &crate::ResolvedElement,
    request: &ExportRequest,
    context: &ExportContext<'_>,
    options: &PreflightOptions,
    report: &mut ValidationReport,
) {
    let Some(layout) = &element.text_layout else {
        return;
    };
    if request.format == ExportFormat::Pdf
        && matches!(request.pdf_mode, PdfMode::Editable | PdfMode::Hybrid)
    {
        for run in layout.lines.iter().flat_map(|line| &line.runs) {
            if context.fonts.get(&run.font).is_err() {
                report.push(
                    ValidationSeverity::Error,
                    ValidationCode::FontEmbedding,
                    Some(page),
                    Some(element.id.as_str()),
                    format!("font `{}` is unavailable for embedding", run.font),
                    options.max_issues,
                );
            }
        }
    }
}

fn inspect_image(
    page: usize,
    element: &crate::ResolvedElement,
    request: &ExportRequest,
    context: &ExportContext<'_>,
    options: &PreflightOptions,
    report: &mut ValidationReport,
) -> Result<()> {
    if options.require_accessibility {
        report.push(
            ValidationSeverity::Warning,
            ValidationCode::Accessibility,
            Some(page),
            Some(element.id.as_str()),
            "schema 1.0 does not carry image alternative text",
            options.max_issues,
        );
    }
    let (Some(name), Some(resolver)) = (&element.asset, context.assets) else {
        report.push(
            ValidationSeverity::Error,
            ValidationCode::Asset,
            Some(page),
            Some(element.id.as_str()),
            "image asset or explicit resolver is missing",
            options.max_issues,
        );
        return Ok(());
    };
    match resolver.resolve_asset(name, context.limits.max_asset_bytes) {
        Ok(asset) => match element.image_placement {
            Some(placement) if !placement.vector => {
                let transformed_destination = element.transform.bounds(placement.destination)?;
                let dpi_x_tenths = effective_dpi_tenths(
                    placement.source.width,
                    transformed_destination.size.width,
                )?;
                let dpi_y_tenths = effective_dpi_tenths(
                    placement.source.height,
                    transformed_destination.size.height,
                )?;
                let dpi_tenths = dpi_x_tenths.min(dpi_y_tenths);
                if dpi_tenths < u64::from(options.minimum_image_dpi) * 10 {
                    report.push(
                        ValidationSeverity::Warning,
                        ValidationCode::Dpi,
                        Some(page),
                        Some(element.id.as_str()),
                        format!(
                            "effective image DPI {}.{} is below {}",
                            dpi_tenths / 10,
                            dpi_tenths % 10,
                            options.minimum_image_dpi
                        ),
                        options.max_issues,
                    );
                }
                if request.format == ExportFormat::Jpeg
                    && crate::validation_capability::raster_asset_has_alpha(&asset, placement)?
                {
                    report.push(
                        ValidationSeverity::Warning,
                        ValidationCode::Capability,
                        Some(page),
                        Some(element.id.as_str()),
                        "JPEG flattens raster image alpha on white",
                        options.max_issues,
                    );
                }
            }
            Some(_) => {
                if matches!(
                    request.format,
                    ExportFormat::Pdf | ExportFormat::Png | ExportFormat::Jpeg
                ) {
                    report.push(
                        ValidationSeverity::Warning,
                        ValidationCode::Capability,
                        Some(page),
                        Some(element.id.as_str()),
                        "requested exporter does not rasterize SVG assets",
                        options.max_issues,
                    );
                }
            }
            None => report.push(
                ValidationSeverity::Error,
                ValidationCode::Asset,
                Some(page),
                Some(element.id.as_str()),
                "image geometry was not resolved during layout",
                options.max_issues,
            ),
        },
        Err(error) => report.push(
            ValidationSeverity::Error,
            ValidationCode::Asset,
            Some(page),
            Some(element.id.as_str()),
            error.to_string(),
            options.max_issues,
        ),
    }
    Ok(())
}

fn inspect_accessibility(
    request: &ExportRequest,
    options: &PreflightOptions,
    report: &mut ValidationReport,
) {
    if options.require_accessibility
        && !(request.format == ExportFormat::Html && request.html_mode == HtmlMode::Semantic)
    {
        report.push(
            ValidationSeverity::Warning,
            ValidationCode::Accessibility,
            None,
            None,
            "requested exporter does not provide tagged semantic reading structure",
            options.max_issues,
        );
    }
}

fn effective_dpi_tenths(pixels: u32, size: crate::Unit) -> Result<u64> {
    if size <= crate::Unit::ZERO {
        return Err(FileMakerError::new(
            ErrorCode::GeometryInvalid,
            "image destination size must be positive",
        ));
    }
    let numerator = u128::from(pixels) * 720 * u128::from(crate::Unit::PER_POINT as u64);
    let denominator = u128::try_from(size.raw()).map_err(|_| {
        FileMakerError::new(
            ErrorCode::GeometryInvalid,
            "image destination size cannot be converted",
        )
    })?;
    let value = numerator / denominator;
    u64::try_from(value).map_err(|_| {
        FileMakerError::new(ErrorCode::GeometryInvalid, "effective image DPI overflow")
    })
}
