// =============================================================================
//        #######
//     ###       ###     F: core.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded core contracts and behavior for this crate.

use std::collections::BTreeSet;
use std::io::Write;

use serde::{Deserialize, Serialize};

use super::progress::ExportProgress;
use crate::{
    AssetResolver, Color, ComputedStyle, ErrorCode, FileMakerError, FontManager, OperationControl,
    ResolvedScene, ResourceLimits, Result,
};

/// Output selected by the export call, never by YAML.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    /// Editable, flattened, or hybrid PDF.
    Pdf,
    /// Vector SVG.
    Svg,
    /// Lossless raster PNG.
    Png,
    /// Lossy raster JPEG.
    Jpeg,
    /// Semantic or fixed HTML.
    Html,
}

/// Caller-selected fidelity behavior.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fidelity {
    /// Reject every unsupported/lossy conversion.
    #[default]
    Strict,
    /// Continue only after recording each loss.
    BestEffort,
}

/// PDF text/graphics mode.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PdfMode {
    /// Retain editable text with subsetted embedded fonts.
    #[default]
    Editable,
    /// Convert text to vector outlines.
    Flattened,
    /// Draw vector outlines and add an invisible searchable text layer.
    Hybrid,
}

/// HTML structure mode.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HtmlMode {
    /// Prefer meaningful HTML elements and reading order.
    #[default]
    Semantic,
    /// Reproduce resolved page geometry with absolute positioning.
    Fixed,
}

/// Paint-only export layer applied after layout without changing geometry.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ExportStyleOverride {
    /// Replacement fill for every resolved element and table cell.
    pub fill: Option<Color>,
    /// Replacement stroke for every resolved element and table cell.
    pub stroke: Option<Color>,
    /// Replacement opacity in millionths.
    pub opacity: Option<u32>,
    /// Replacement text foreground.
    pub color: Option<Color>,
}

impl ExportStyleOverride {
    fn validate(self) -> Result<()> {
        for color in [self.fill, self.stroke, self.color].into_iter().flatten() {
            color.validate()?;
        }
        if self.opacity.is_some_and(|opacity| opacity > 1_000_000) {
            return Err(FileMakerError::new(
                ErrorCode::ExportUnsupported,
                "export style opacity must be at most 1000000",
            ));
        }
        Ok(())
    }

    fn apply(self, style: &mut ComputedStyle) {
        if self.fill.is_some() {
            style.fill = self.fill;
        }
        if self.stroke.is_some() {
            style.stroke = self.stroke;
        }
        if let Some(opacity) = self.opacity {
            style.opacity = opacity;
        }
        if let Some(color) = self.color {
            style.color = color;
        }
    }
}

/// One complete export request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExportRequest {
    /// Output format.
    pub format: ExportFormat,
    /// Strict or reported-loss behavior.
    pub fidelity: Fidelity,
    /// Optional zero-based page; absent exports all pages where supported.
    pub page: Option<usize>,
    /// Raster DPI, ignored by vector exporters.
    pub dpi: u32,
    /// JPEG quality from 1 through 100.
    pub jpeg_quality: u8,
    /// PDF mode.
    pub pdf_mode: PdfMode,
    /// HTML mode.
    pub html_mode: HtmlMode,
    /// Optional final paint layer; geometry-affecting style is intentionally absent.
    #[serde(default)]
    pub style_override: Option<ExportStyleOverride>,
}

impl Default for ExportRequest {
    fn default() -> Self {
        Self {
            format: ExportFormat::Svg,
            fidelity: Fidelity::Strict,
            page: None,
            dpi: 144,
            jpeg_quality: 90,
            pdf_mode: PdfMode::Editable,
            html_mode: HtmlMode::Semantic,
            style_override: None,
        }
    }
}

/// Exporter feature declaration.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportCapabilities {
    /// Multiple pages in one output.
    MultiPage,
    /// Editable text.
    EditableText,
    /// Embedded fonts.
    EmbeddedFonts,
    /// Vector geometry.
    Vector,
    /// Raster geometry.
    Raster,
    /// Alpha transparency.
    Transparency,
    /// Native CMYK.
    Cmyk,
    /// Embedded raster/vector images.
    Images,
    /// Semantic reading structure.
    Semantic,
    /// Deterministic document metadata.
    Metadata,
}

/// Stable reason a feature could not be preserved exactly.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportLossKind {
    /// Only one page could be represented.
    AdditionalPagesOmitted,
    /// CMYK was converted to RGB.
    CmykConvertedToRgb,
    /// Alpha was composited or removed.
    TransparencyFlattened,
    /// Semantic structure was replaced with fixed geometry.
    SemanticsFlattened,
    /// Text was converted to outlines/pixels.
    TextFlattened,
    /// An element kind is only prepared, not renderable.
    UnsupportedElement,
    /// Image could not be represented.
    ImageOmitted,
    /// A prepared text capability could not be represented.
    TextCapabilityUnsupported,
}

/// One bounded loss item.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExportLoss {
    /// Loss kind.
    pub kind: ExportLossKind,
    /// Optional element ID.
    pub element: Option<String>,
    /// Bounded explanation.
    pub message: String,
}

/// First-class accumulated loss report.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExportLossReport {
    /// Losses in deterministic discovery order.
    pub losses: Vec<ExportLoss>,
}

impl ExportLossReport {
    /// Records a bounded loss.
    pub fn push(
        &mut self,
        kind: ExportLossKind,
        element: Option<&str>,
        message: impl Into<String>,
    ) {
        let mut message = message.into();
        message.truncate(512);
        self.losses.push(ExportLoss {
            kind,
            element: element.map(ToOwned::to_owned),
            message,
        });
    }

    /// Rejects a non-empty report under strict fidelity.
    pub fn enforce(&self, fidelity: Fidelity) -> Result<()> {
        if fidelity == Fidelity::Strict && !self.losses.is_empty() {
            let first = &self.losses[0];
            return Err(FileMakerError::new(
                ErrorCode::ExportUnsupported,
                format!("strict export rejected {:?}: {}", first.kind, first.message),
            ));
        }
        Ok(())
    }
}

pub(super) fn record_text_capability_losses(
    element: &crate::ResolvedElement,
    losses: &mut ExportLossReport,
) {
    for diagnostic in text_layouts(element)
        .into_iter()
        .flat_map(|layout| &layout.diagnostics)
    {
        let message = match diagnostic {
            crate::TextDiagnostic::VerticalWritingUnavailable => {
                Some("vertical writing is a prepared capability")
            }
            crate::TextDiagnostic::ColorEmojiRequiresExporter => {
                Some("color emoji requires an exporter-specific implementation")
            }
            crate::TextDiagnostic::Clipped
            | crate::TextDiagnostic::Ellipsized
            | crate::TextDiagnostic::Shrunk => None,
        };
        if let Some(message) = message {
            losses.push(
                ExportLossKind::TextCapabilityUnsupported,
                Some(element.id.as_str()),
                message,
            );
        }
    }
}

pub(super) fn text_layouts(element: &crate::ResolvedElement) -> Vec<&crate::TextLayout> {
    let mut layouts = Vec::new();
    if let Some(layout) = &element.text_layout {
        layouts.push(layout);
    }
    if let Some(table) = &element.table {
        layouts.extend(table.header.iter().map(|cell| &cell.text_layout));
        layouts.extend(
            table
                .rows
                .iter()
                .flat_map(|row| &row.cells)
                .map(|cell| &cell.text_layout),
        );
        layouts.extend(table.totals.iter().map(|cell| &cell.text_layout));
    }
    layouts
}

pub(super) fn text_fonts(element: &crate::ResolvedElement) -> Vec<&str> {
    text_layouts(element)
        .into_iter()
        .flat_map(|layout| &layout.lines)
        .flat_map(|line| &line.runs)
        .map(|run| run.font.as_str())
        .collect()
}

/// Explicit resources available during export.
pub struct ExportContext<'a> {
    /// Resource limits.
    pub limits: &'a ResourceLimits,
    /// Explicit font registry.
    pub fonts: &'a FontManager,
    /// Optional explicit asset resolver.
    pub assets: Option<&'a dyn AssetResolver>,
}

/// Completed export metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExportOutcome {
    /// Bytes written to the caller's writer.
    pub bytes_written: usize,
    /// Explicit loss report.
    pub loss_report: ExportLossReport,
    /// Declared exporter capabilities.
    pub capabilities: BTreeSet<ExportCapabilities>,
}

/// Dispatches to the exact requested exporter.
pub fn export(
    scene: &ResolvedScene,
    request: &ExportRequest,
    context: &ExportContext<'_>,
    writer: &mut dyn Write,
) -> Result<ExportOutcome> {
    export_with_control(scene, request, context, None, writer)
}

fn export_with_control(
    scene: &ResolvedScene,
    request: &ExportRequest,
    context: &ExportContext<'_>,
    control: Option<&OperationControl>,
    writer: &mut dyn Write,
) -> Result<ExportOutcome> {
    validate_request(scene, request, context.limits)?;
    let mut progress = ExportProgress::new(scene, request, control)?;
    if let Some(style) = request.style_override {
        let mut scene = scene.clone();
        apply_export_style(&mut scene, style);
        let outcome = export_prepared(&scene, request, context, &mut progress, writer)?;
        progress.finish()?;
        return Ok(outcome);
    }
    let outcome = export_prepared(scene, request, context, &mut progress, writer)?;
    progress.finish()?;
    Ok(outcome)
}

/// Exports into a bounded in-memory byte vector.
pub fn export_bytes(
    scene: &ResolvedScene,
    request: &ExportRequest,
    context: &ExportContext<'_>,
) -> Result<(Vec<u8>, ExportOutcome)> {
    let mut bytes = Vec::new();
    let outcome = export(scene, request, context, &mut bytes)?;
    Ok((bytes, outcome))
}

fn export_prepared(
    scene: &ResolvedScene,
    request: &ExportRequest,
    context: &ExportContext<'_>,
    progress: &mut ExportProgress<'_>,
    writer: &mut dyn Write,
) -> Result<ExportOutcome> {
    match request.format {
        ExportFormat::Svg => super::svg::export(scene, request, context, progress, writer),
        ExportFormat::Png | ExportFormat::Jpeg => {
            super::raster::export(scene, request, context, progress, writer)
        }
        ExportFormat::Html => super::html::export(scene, request, context, progress, writer),
        ExportFormat::Pdf => super::pdf::export(scene, request, context, progress, writer),
    }
}

fn apply_export_style(scene: &mut ResolvedScene, style: ExportStyleOverride) {
    for page in &mut scene.pages {
        for element in &mut page.elements {
            style.apply(&mut element.style);
            if let Some(table) = &mut element.table {
                for cell in table
                    .header
                    .iter_mut()
                    .chain(table.rows.iter_mut().flat_map(|row| {
                        style.apply(&mut row.style);
                        row.cells.iter_mut()
                    }))
                    .chain(table.totals.iter_mut())
                {
                    style.apply(&mut cell.style);
                }
            }
        }
    }
}

/// Exports with cancellation checks and progress at page/element boundaries.
pub fn export_controlled(
    scene: &ResolvedScene,
    request: &ExportRequest,
    context: &ExportContext<'_>,
    control: &OperationControl,
    writer: &mut dyn Write,
) -> Result<ExportOutcome> {
    export_with_control(scene, request, context, Some(control), writer)
}

pub(super) fn selected_pages<'a>(
    scene: &'a ResolvedScene,
    request: &ExportRequest,
) -> Result<Vec<&'a crate::ResolvedPage>> {
    if let Some(index) = request.page {
        return scene
            .pages
            .get(index)
            .map(|page| vec![page])
            .ok_or_else(|| {
                FileMakerError::new(ErrorCode::ExportUnsupported, "requested page was not found")
            });
    }
    Ok(scene.pages.iter().collect())
}

pub(crate) fn validate_request(
    scene: &ResolvedScene,
    request: &ExportRequest,
    limits: &ResourceLimits,
) -> Result<()> {
    crate::resolved::validate_scene_contract(scene, limits)?;
    if let Some(style) = request.style_override {
        style.validate()?;
    }
    let invalid_dpi = matches!(request.format, ExportFormat::Png | ExportFormat::Jpeg)
        && (request.dpi == 0 || request.dpi > 9_600);
    let invalid_quality = request.format == ExportFormat::Jpeg
        && (request.jpeg_quality == 0 || request.jpeg_quality > 100);
    if scene.pages.is_empty() || invalid_dpi || invalid_quality {
        return Err(FileMakerError::new(
            ErrorCode::ExportUnsupported,
            "export request or scene is invalid",
        ));
    }
    Ok(())
}
