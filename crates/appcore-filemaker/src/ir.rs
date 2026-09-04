// =============================================================================
//        #######
//     ###       ###     F: ir.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded ir contracts and behavior for this crate.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::source::ModelKind;
use crate::{
    CollisionPolicy, DataSchema, ErrorCode, FileMakerError, ImageOptions, Length, Orientation,
    PageTemplate, Result, Size, Style,
};

/// Validated stable element identifier.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash, Serialize)]
#[serde(transparent)]
pub struct ElementId(String);

impl ElementId {
    /// Validates and creates an identifier.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 128
            || !value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'/')
            })
        {
            return Err(FileMakerError::new(
                ErrorCode::SchemaField,
                "element ID must be 1..128 safe ASCII characters",
            ));
        }
        Ok(Self(value))
    }

    /// Returns the validated ID.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ElementId {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// Format-neutral element kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElementKind {
    /// Text.
    Text,
    /// Image.
    Image,
    /// Line.
    Line,
    /// Rectangle.
    Rect,
    /// Circle.
    Circle,
    /// Ellipse.
    Ellipse,
    /// Polygon.
    Polygon,
    /// Vector path.
    Path,
    /// Group.
    Group,
    /// First-class table.
    Table,
    /// Reserved chart node.
    Chart,
    /// Reserved QR node.
    Qr,
    /// Reserved barcode node.
    Barcode,
}

impl ElementKind {
    /// Parses the exact schema spelling.
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "text" => Ok(Self::Text),
            "image" => Ok(Self::Image),
            "line" => Ok(Self::Line),
            "rect" => Ok(Self::Rect),
            "circle" => Ok(Self::Circle),
            "ellipse" => Ok(Self::Ellipse),
            "polygon" => Ok(Self::Polygon),
            "path" => Ok(Self::Path),
            "group" => Ok(Self::Group),
            "table" => Ok(Self::Table),
            "chart" => Ok(Self::Chart),
            "qr" => Ok(Self::Qr),
            "barcode" => Ok(Self::Barcode),
            _ => Err(FileMakerError::new(
                ErrorCode::SchemaField,
                format!("unsupported element type `{value}`"),
            )),
        }
    }
}

/// Source-independent geometry intent retained before measurement.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GeometryIr {
    /// Optional horizontal position.
    pub x: Option<Length>,
    /// Optional vertical position.
    pub y: Option<Length>,
    /// Optional/automatic width.
    pub width: Option<Length>,
    /// Optional/automatic height.
    pub height: Option<Length>,
    /// Minimum, preferred, maximum, and aspect-ratio size intent.
    pub constraints: crate::LayoutConstraints,
    /// Optional horizontal alignment inside the resolved container.
    pub align_x: Option<crate::Alignment>,
    /// Optional vertical alignment inside the resolved container.
    pub align_y: Option<crate::Alignment>,
    /// Named containing region.
    pub region: Option<String>,
    /// Named anchor expressions.
    pub anchors: BTreeMap<String, String>,
}

/// Source-independent transform intent retained until page geometry is known.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TransformIr {
    /// Horizontal translation.
    pub translate_x: Length,
    /// Vertical translation.
    pub translate_y: Length,
    /// Clockwise integer-degree rotation.
    pub rotate: i32,
    /// Horizontal fixed-point scale.
    pub scale_x: i64,
    /// Vertical fixed-point scale.
    pub scale_y: i64,
    /// Horizontal transform origin.
    pub origin_x: Length,
    /// Vertical transform origin.
    pub origin_y: Length,
}

/// Format-neutral text layout intent retained until font measurement.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TextIr {
    /// Overflow behavior.
    pub overflow: crate::TextOverflow,
    /// Optional maximum line count.
    pub max_lines: Option<usize>,
    /// Optional explicit minimum font size.
    pub min_font_size: Option<Length>,
    /// Line-height multiplier in millionths.
    pub line_height: u32,
    /// Horizontal lines or top-to-bottom right-to-left vertical columns.
    pub writing_mode: crate::WritingMode,
}

/// Bound table intent retained until row/column measurement and pagination.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TableIr {
    /// Validated planning contract.
    pub spec: crate::TableSpec,
    /// Header height in source-relative units.
    pub header_height: Length,
    /// Fixed row height or auto measurement.
    pub row_height: Option<Length>,
    /// Bound bounded rows; empty before data binding.
    pub rows: Vec<crate::DataRow>,
}

/// Source-relative vector path command retained until layout resolves its units.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum PathCommandIr {
    /// Starts a contour.
    Move { x: Length, y: Length },
    /// Adds a straight segment.
    Line { x: Length, y: Length },
    /// Adds a cubic Bézier segment.
    Curve {
        x1: Length,
        y1: Length,
        x2: Length,
        y2: Length,
        x: Length,
        y: Length,
    },
    /// Closes the current contour.
    Close,
}

/// Named region retained in source-relative units until page layout.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RegionIr {
    /// Horizontal position.
    pub x: Length,
    /// Vertical position.
    pub y: Length,
    /// Width.
    pub width: Length,
    /// Height.
    pub height: Length,
    /// Region collision policy inherited after document and page policies.
    pub collision: Option<crate::CollisionPolicy>,
}

/// Page-relative non-painted geometry retained until layout.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExclusionIr {
    /// Horizontal coordinate.
    pub x: Length,
    /// Vertical coordinate.
    pub y: Length,
    /// Width.
    pub width: Length,
    /// Height.
    pub height: Length,
    /// Collision group exposed by the exclusion.
    pub group: String,
    /// Candidate groups blocked by this exclusion; empty means every group.
    pub collides_with: BTreeSet<String>,
}

/// Layout strategy for a node and its children.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutMode {
    /// Coordinates are resolved independently.
    #[default]
    Absolute,
    /// Children flow from top to bottom.
    FlowVertical,
    /// Children flow from left to right.
    FlowHorizontal,
}

/// Provenance retained through binding and layout.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    /// Logical source path.
    pub source: String,
    /// Component expansion chain.
    pub components: Vec<String>,
    /// Applied style names in cascade order.
    pub styles: Vec<String>,
    /// Runtime patch sequence numbers.
    pub patches: Vec<u64>,
}

/// Author-supplied edit policy carried for optional external tool bridges.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AiPolicy {
    /// Compact document purpose; the deterministic core does not interpret it.
    pub purpose: String,
    /// Bounded textual rules; the deterministic core does not interpret them.
    pub rules: Vec<String>,
    /// IDs an external bridge may edit; empty delegates to bridge defaults.
    pub editable: BTreeSet<String>,
    /// IDs an external bridge must never edit.
    pub locked: BTreeSet<String>,
}

/// Expanded format-neutral element.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ElementIr {
    /// Stable ID.
    pub id: ElementId,
    /// Element kind.
    pub kind: ElementKind,
    /// Geometry intent.
    pub geometry: GeometryIr,
    /// Transform intent resolved only after layout proposes a box.
    pub transform: TransformIr,
    /// Literal or bound text.
    pub text: Option<String>,
    /// Text measurement and overflow intent.
    pub text_options: TextIr,
    /// First-class table planning intent and bound rows.
    pub table: Option<TableIr>,
    /// Explicit asset reference.
    pub asset: Option<String>,
    /// Vector commands in element-local source coordinates.
    pub path: Vec<PathCommandIr>,
    /// Image crop and fit intent.
    pub image: ImageOptions,
    /// Typed inline/cascaded style.
    pub style: Style,
    /// Ordered conditional style layers evaluated during data binding.
    #[serde(default)]
    pub style_rules: Vec<crate::ElementStyleRule>,
    /// Layout strategy.
    pub layout: LayoutMode,
    /// Distribution of children on a flow's primary axis.
    pub distribute: crate::Distribution,
    /// Gap between flow children.
    pub gap: Length,
    /// Optional policy overriding the inherited collision policy.
    pub collision: Option<CollisionPolicy>,
    /// Child elements in deterministic source order.
    pub children: Vec<ElementIr>,
    /// Immutable after compile unless a privileged caller builds a new IR.
    pub locked: bool,
    /// Visibility after binding rules.
    pub hidden: bool,
    /// Visual layer independent of collision.
    pub layer: String,
    /// Visual order within layer.
    pub z_index: i32,
    /// Data binding expression.
    pub binding: Option<String>,
    /// Conditional expression.
    pub when: Option<String>,
    /// Repeat expression.
    pub repeat: Option<String>,
    /// Provenance.
    pub provenance: Provenance,
    /// Root-only master/role page placement; children inherit their root.
    pub page_placement: Option<crate::PagePlacement>,
}

/// Expanded reusable template IR.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TemplateIr {
    /// Template identity.
    pub id: String,
    /// Model.
    pub model: ModelKind,
    /// Optional page size.
    pub page_size: Option<Size>,
    /// Source orientation.
    pub orientation: Orientation,
    /// Resolved trim, margin, bleed, safe-area, and crop metadata.
    pub page_template: Option<PageTemplate>,
    /// Document-wide collision policy.
    pub collision: Option<crate::CollisionPolicy>,
    /// Page collision policy inherited after the document policy.
    pub page_collision: Option<crate::CollisionPolicy>,
    /// Named guides.
    pub guides: BTreeMap<String, Length>,
    /// Named regions.
    pub regions: BTreeMap<String, RegionIr>,
    /// Named page-relative exclusions.
    pub exclusions: BTreeMap<String, ExclusionIr>,
    /// Optional typed data contract.
    pub data_schema: DataSchema,
    /// Optional external-tool edit policy, retained but never executed by core.
    pub ai_policy: AiPolicy,
    /// Expanded root nodes.
    pub elements: Vec<ElementIr>,
}

/// Bound instance ready for measurement and layout.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DocumentIr {
    /// Template identity.
    pub template_id: String,
    /// Model.
    pub model: ModelKind,
    /// Optional page size.
    pub page_size: Option<Size>,
    /// Resolved page metadata.
    pub page_template: Option<PageTemplate>,
    /// Bound document-wide collision policy.
    pub collision: Option<crate::CollisionPolicy>,
    /// Bound page collision policy inherited after the document policy.
    pub page_collision: Option<crate::CollisionPolicy>,
    /// Named guides.
    pub guides: BTreeMap<String, Length>,
    /// Named regions.
    pub regions: BTreeMap<String, RegionIr>,
    /// Named page-relative exclusions.
    pub exclusions: BTreeMap<String, ExclusionIr>,
    /// Optional external-tool edit policy, retained but never executed by core.
    pub ai_policy: AiPolicy,
    /// Bound root nodes.
    pub elements: Vec<ElementIr>,
}

impl TemplateIr {
    /// Verifies global ID uniqueness and a caller-supplied element bound.
    pub fn validate(&self, max_elements: usize) -> Result<()> {
        let mut ids = BTreeSet::new();
        let mut count = self.exclusions.len();
        if count > max_elements {
            return Err(FileMakerError::new(
                ErrorCode::LimitExceeded,
                format!("element and exclusion count exceeds {max_elements}"),
            ));
        }
        for (name, exclusion) in &self.exclusions {
            validate_exclusion(name, exclusion)?;
        }
        let mut stack: Vec<&ElementIr> = self.elements.iter().rev().collect();
        while let Some(element) = stack.pop() {
            count = count.saturating_add(1);
            if count > max_elements {
                return Err(FileMakerError::new(
                    ErrorCode::LimitExceeded,
                    format!("element count exceeds {max_elements}"),
                ));
            }
            if !ids.insert(element.id.as_str()) {
                return Err(FileMakerError::new(
                    ErrorCode::SchemaField,
                    format!("duplicate element ID `{}`", element.id.as_str()),
                ));
            }
            stack.extend(element.children.iter().rev());
        }
        Ok(())
    }
}

fn validate_exclusion(name: &str, exclusion: &ExclusionIr) -> Result<()> {
    validate_exclusion_name("exclusion", name, 118)?;
    validate_exclusion_name("exclusion group", &exclusion.group, 128)?;
    if exclusion.collides_with.len() > 64 {
        return Err(FileMakerError::new(
            ErrorCode::LimitExceeded,
            "exclusion collision-group list exceeds 64",
        ));
    }
    for group in &exclusion.collides_with {
        validate_exclusion_name("exclusion collision group", group, 128)?;
    }
    if [exclusion.x, exclusion.y, exclusion.width, exclusion.height].contains(&Length::Auto) {
        return Err(FileMakerError::new(
            ErrorCode::SchemaField,
            "exclusion geometry cannot be auto",
        ));
    }
    Ok(())
}

fn validate_exclusion_name(label: &str, value: &str, max_bytes: usize) -> Result<()> {
    if value.is_empty()
        || value.len() > max_bytes
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(FileMakerError::new(
            ErrorCode::SchemaField,
            format!("{label} name is invalid"),
        ));
    }
    Ok(())
}
