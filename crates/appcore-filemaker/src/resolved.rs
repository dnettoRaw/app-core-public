// =============================================================================
//        #######
//     ###       ###     F: resolved.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded resolved contracts and behavior for this crate.

use serde::{Deserialize, Serialize};

use crate::{
    BoundsSet, CollisionPolicy, ComputedStyle, ElementId, ElementKind, ErrorCode, FileMakerError,
    GeometryIr, ImagePlacement, PageTemplate, Provenance, Rect, ResolvedTableFragment,
    ResourceLimits, Result, Shape, Size, TextLayout, Transform,
};

/// Geometry and collision inputs retained for deterministic layout explanation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LayoutTrace {
    /// Position, size, constraints, region, and anchors from the bound IR.
    pub geometry: GeometryIr,
    /// Proposed layout rectangle before collision or page reflow.
    pub proposed: Rect,
    /// Effective inherited collision policy.
    pub collision_policy: CollisionPolicy,
    /// Page proposed before pagination or collision reflow.
    pub initial_page: usize,
    /// Whether collision or pagination changed the proposed placement.
    pub reflowed: bool,
}

/// Named region resolved into page coordinates for inspection and overlays.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedRegion {
    /// Stable region name.
    pub name: String,
    /// Final page-local bounds.
    pub bounds: Rect,
}

/// Named non-painted page geometry retained for inspection and collision masks.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedExclusion {
    /// Stable source name.
    pub name: String,
    /// Final page-local bounds.
    pub bounds: crate::Rect,
    /// Collision group exposed by this geometry.
    pub group: String,
    /// Candidate groups blocked by this exclusion; empty means every group.
    pub collides_with: std::collections::BTreeSet<String>,
}

/// Fully resolved exporter-facing element.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedElement {
    /// Stable source ID.
    pub id: ElementId,
    /// Element kind.
    pub kind: ElementKind,
    /// Distinct resolved geometry boxes.
    pub bounds: BoundsSet,
    /// Whether this element participates in collision masks and preflight.
    pub collidable: bool,
    /// Collision/vector shape.
    pub shape: Shape,
    /// Transform already accounted for in collision bounds.
    pub transform: Transform,
    /// Computed style.
    pub style: ComputedStyle,
    /// Literal/bound text.
    pub text: Option<String>,
    /// Shaped glyph layout when this is text.
    pub text_layout: Option<TextLayout>,
    /// Explicit asset name.
    pub asset: Option<String>,
    /// Exporter-ready image crop and destination geometry.
    pub image_placement: Option<ImagePlacement>,
    /// Exporter-ready table fragment geometry and shaped cell text.
    pub table: Option<ResolvedTableFragment>,
    /// Visual layer.
    pub layer: String,
    /// Visual z index.
    pub z_index: i32,
    /// Stable source order.
    pub sequence: usize,
    /// Source and mutation provenance.
    pub provenance: Provenance,
    /// Bound source geometry and placement decisions.
    pub layout_trace: LayoutTrace,
}

/// One resolved page/canvas.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedPage {
    /// Zero-based page index.
    pub index: usize,
    /// First, continuation, or last physical-page role.
    pub role: crate::PageRole,
    /// Resolved page size.
    pub size: Size,
    /// Trim, margin, bleed, safe-area, and crop metadata.
    pub page_template: Option<PageTemplate>,
    /// Non-painted geometry seeded into this page's collision index.
    pub exclusions: Vec<ResolvedExclusion>,
    /// Named page geometry retained for debug overlays and inspection.
    pub regions: Vec<ResolvedRegion>,
    /// Elements sorted by layer, z index, and source sequence for painting.
    pub elements: Vec<ResolvedElement>,
}

/// Complete immutable scene consumed by exporters and inspection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedScene {
    /// Template identity.
    pub template_id: String,
    /// Pages/canvases in order.
    pub pages: Vec<ResolvedPage>,
    /// Engine version that resolved the scene.
    pub engine_version: String,
}

pub(crate) fn validate_scene_contract(
    scene: &ResolvedScene,
    limits: &ResourceLimits,
) -> Result<()> {
    limits.validate()?;
    if scene.engine_version != crate::ENGINE_VERSION {
        return Err(FileMakerError::new(
            ErrorCode::Validation,
            "resolved scene engine version does not match the active engine",
        ));
    }
    if scene.pages.len() > limits.max_pages {
        return Err(limit_error("resolved scene exceeds the page budget"));
    }
    if scene.template_id.is_empty() || scene.template_id.len() > limits.max_text_bytes {
        return Err(contract_error(
            "resolved scene template identity is invalid",
        ));
    }
    let mut counts = SceneCounts::default();
    for (expected_page, page) in scene.pages.iter().enumerate() {
        validate_page(page, expected_page, scene.pages.len(), limits, &mut counts)?;
    }
    Ok(())
}

#[derive(Default)]
struct SceneCounts {
    elements: usize,
    paths: usize,
    rows: u64,
}

fn validate_page(
    page: &ResolvedPage,
    expected_page: usize,
    total_pages: usize,
    limits: &ResourceLimits,
    counts: &mut SceneCounts,
) -> Result<()> {
    if page.index != expected_page
        || page.role != expected_page_role(expected_page, total_pages)
        || page.size.width <= crate::Unit::ZERO
        || page.size.height <= crate::Unit::ZERO
    {
        return Err(contract_error("resolved page order or size is invalid"));
    }
    if let Some(template) = &page.page_template {
        if template.role != page.role
            || template.size != page.size
            || template.name.is_empty()
            || template.name.len() > limits.max_text_bytes
        {
            return Err(contract_error(
                "resolved page template metadata is inconsistent",
            ));
        }
        template.content_bounds()?;
        template.safe_bounds()?;
    }
    counts.elements = counts
        .elements
        .checked_add(page.elements.len())
        .ok_or_else(|| limit_error("resolved element count overflow"))?;
    if counts.elements > limits.max_elements
        || page.exclusions.len() > limits.max_elements
        || page.regions.len() > limits.max_elements
    {
        return Err(limit_error("resolved scene exceeds a geometry budget"));
    }
    for exclusion in &page.exclusions {
        validate_rect(exclusion.bounds)?;
    }
    for region in &page.regions {
        validate_rect(region.bounds)?;
    }
    let mut ids = std::collections::BTreeSet::new();
    for element in &page.elements {
        if !ids.insert(element.id.as_str()) || element.layer.len() > 128 {
            return Err(contract_error(
                "resolved element IDs must be unique and layers bounded",
            ));
        }
        validate_bounds(element.bounds)?;
        element.transform.bounds(element.bounds.layout)?;
        element.style.validate()?;
        validate_text(
            element.text.as_deref(),
            element.text_layout.as_ref(),
            limits,
        )?;
        let points = validate_shape(&element.shape)?;
        counts.paths = counts
            .paths
            .checked_add(points)
            .ok_or_else(|| limit_error("resolved path count overflow"))?;
        if counts.paths > limits.max_path_commands {
            return Err(limit_error("resolved scene exceeds the path budget"));
        }
        if let Some(table) = &element.table {
            validate_table(table, limits, &mut counts.rows)?;
        }
        if let Some(placement) = element.image_placement {
            validate_image(placement)?;
        }
    }
    Ok(())
}

fn validate_shape(shape: &Shape) -> Result<usize> {
    let points = match shape {
        Shape::Path { bounds, commands } => {
            validate_rect(*bounds)?;
            if commands.is_empty() {
                return Err(contract_error("resolved path has no commands"));
            }
            commands.len()
        }
        Shape::Polygon { points } => {
            if points.len() < 3 {
                return Err(contract_error(
                    "resolved polygon requires at least three points",
                ));
            }
            points.len()
        }
        Shape::Rect { bounds } | Shape::Ellipse { bounds } => {
            validate_rect(*bounds)?;
            0
        }
    };
    validate_rect(shape.bounds()?)?;
    Ok(points)
}

fn validate_table(
    table: &ResolvedTableFragment,
    limits: &ResourceLimits,
    rows: &mut u64,
) -> Result<()> {
    const MAX_COLUMNS: usize = 1_024;
    *rows = rows
        .checked_add(
            u64::try_from(table.rows.len())
                .map_err(|_| limit_error("resolved table row count overflow"))?,
        )
        .ok_or_else(|| limit_error("resolved table row count overflow"))?;
    if *rows > limits.max_rows
        || table.columns.len() > MAX_COLUMNS
        || table.header.len() > MAX_COLUMNS
        || table.totals.len() > MAX_COLUMNS
        || table.rows.iter().any(|row| row.cells.len() > MAX_COLUMNS)
    {
        return Err(limit_error(
            "resolved table exceeds its row or column budget",
        ));
    }
    if table.columns.iter().any(|column| {
        column.width <= crate::Unit::ZERO
            || column.field.is_empty()
            || column.field.len() > limits.max_text_bytes
            || column.header.len() > limits.max_text_bytes
    }) {
        return Err(contract_error("resolved table column is invalid"));
    }
    for cell in table
        .header
        .iter()
        .chain(table.rows.iter().flat_map(|row| &row.cells))
        .chain(&table.totals)
    {
        validate_rect(cell.bounds)?;
        cell.style.validate()?;
        validate_text(Some(&cell.text), Some(&cell.text_layout), limits)?;
    }
    for row in &table.rows {
        validate_rect(row.bounds)?;
        row.style.validate()?;
    }
    Ok(())
}

fn validate_bounds(bounds: BoundsSet) -> Result<()> {
    for bounds in [
        bounds.intrinsic,
        bounds.layout,
        bounds.collision,
        bounds.visual,
    ] {
        validate_rect(bounds)?;
    }
    if let Some(clip) = bounds.clip {
        validate_rect(clip)?;
    }
    Ok(())
}

fn validate_rect(rect: Rect) -> Result<()> {
    if rect.size.width < crate::Unit::ZERO || rect.size.height < crate::Unit::ZERO {
        return Err(contract_error("resolved rectangle has a negative size"));
    }
    rect.right()?;
    rect.bottom()?;
    Ok(())
}

fn validate_image(placement: ImagePlacement) -> Result<()> {
    validate_rect(placement.destination)?;
    validate_rect(placement.clip)?;
    let right = placement
        .source
        .x
        .checked_add(placement.source.width)
        .ok_or_else(|| contract_error("resolved image source width overflow"))?;
    let bottom = placement
        .source
        .y
        .checked_add(placement.source.height)
        .ok_or_else(|| contract_error("resolved image source height overflow"))?;
    if placement.source.width == 0
        || placement.source.height == 0
        || placement.intrinsic_width == 0
        || placement.intrinsic_height == 0
        || right > placement.intrinsic_width
        || bottom > placement.intrinsic_height
    {
        return Err(contract_error("resolved image placement is invalid"));
    }
    Ok(())
}

fn validate_text(
    text: Option<&str>,
    layout: Option<&TextLayout>,
    limits: &ResourceLimits,
) -> Result<()> {
    if text.is_some_and(|value| value.len() > limits.max_text_bytes) {
        return Err(limit_error("resolved text exceeds the byte budget"));
    }
    let Some(layout) = layout else {
        return Ok(());
    };
    if layout.measured.width < crate::Unit::ZERO
        || layout.measured.height < crate::Unit::ZERO
        || layout.font_size <= crate::Unit::ZERO
        || layout.lines.len() > limits.max_text_bytes
        || layout.diagnostics.len() > limits.max_text_bytes
    {
        return Err(contract_error("resolved text layout geometry is invalid"));
    }
    let mut bytes = 0_usize;
    let mut glyphs = 0_usize;
    let mut runs = 0_usize;
    for line in &layout.lines {
        if line.width < crate::Unit::ZERO || line.height <= crate::Unit::ZERO {
            return Err(contract_error("resolved text line geometry is invalid"));
        }
        runs = runs
            .checked_add(line.runs.len())
            .ok_or_else(|| limit_error("resolved text run count overflow"))?;
        for run in &line.runs {
            if run.font.is_empty()
                || run.font.len() > 128
                || run.width < crate::Unit::ZERO
                || run.glyphs.iter().any(|glyph| {
                    usize::try_from(glyph.cluster).map_or(true, |index| index > run.text.len())
                })
            {
                return Err(contract_error("resolved glyph run is invalid"));
            }
            bytes = bytes
                .checked_add(run.text.len())
                .ok_or_else(|| limit_error("resolved text byte count overflow"))?;
            glyphs = glyphs
                .checked_add(run.glyphs.len())
                .ok_or_else(|| limit_error("resolved glyph count overflow"))?;
        }
    }
    if bytes > limits.max_text_bytes
        || glyphs > limits.max_text_bytes
        || runs > limits.max_text_bytes
    {
        return Err(limit_error("resolved text layout exceeds its budget"));
    }
    Ok(())
}

fn expected_page_role(index: usize, total: usize) -> crate::PageRole {
    if index == 0 {
        crate::PageRole::First
    } else if index + 1 == total {
        crate::PageRole::Last
    } else {
        crate::PageRole::Continuation
    }
}

fn limit_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::LimitExceeded, message)
}

fn contract_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::Validation, message)
}
