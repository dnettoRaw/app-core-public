// =============================================================================
//        #######
//     ###       ###     F: source.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded source contracts and behavior for this crate.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::source_layout::default_gap;
pub use crate::source_layout::ExclusionSource;
pub use crate::source_page::{EdgeSource, PageLayerSource, PageSource};
pub use crate::source_table::{TableSource, TableStyleRuleSource};
pub use crate::source_text::TextSourceOptions;
pub use crate::source_transform::{MirrorSource, TransformSource};

use crate::{
    Alignment, CollisionBounds, Color, Distribution, ImageOptions, LayoutConstraints, LayoutMode,
    Length,
};

/// Top-level source model.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelKind {
    /// Paginated document/report.
    Document,
    /// Free-form vector canvas.
    Canvas,
    /// Tabular dataset.
    Dataset,
}

/// Version-one YAML frontend. This is never renderer input.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateSourceV1 {
    /// Must equal `"1.0"`.
    pub filemaker: String,
    /// Source model.
    pub model: ModelKind,
    /// Stable logical template ID.
    pub id: String,
    /// Optional page/canvas declaration.
    #[serde(default)]
    pub page: Option<PageSource>,
    /// Document-wide collision policy inherited by every page.
    #[serde(default)]
    pub collision: Option<CollisionSource>,
    /// Explicit includes expanded before IR construction.
    #[serde(default)]
    pub includes: Vec<IncludeSource>,
    /// Reusable component declarations.
    #[serde(default)]
    pub components: BTreeMap<String, ComponentSource>,
    /// Theme token declarations.
    #[serde(default)]
    pub themes: BTreeMap<String, ThemeSource>,
    /// Explicit active theme name.
    #[serde(default)]
    pub theme: Option<String>,
    /// Template-level style applied after the active theme.
    #[serde(default)]
    pub style: StyleSource,
    /// Named styles.
    #[serde(default)]
    pub styles: BTreeMap<String, StyleSource>,
    /// Named guides.
    #[serde(default)]
    pub guides: BTreeMap<String, Length>,
    /// Named layout regions.
    #[serde(default)]
    pub regions: BTreeMap<String, RegionSource>,
    /// Named non-painted collision geometry repeated on every page.
    #[serde(default)]
    pub exclusions: BTreeMap<String, ExclusionSource>,
    /// Typed input data schema.
    #[serde(default)]
    pub data_schema: BTreeMap<String, DataFieldSource>,
    /// Root elements in stable source order.
    #[serde(default)]
    pub elements: Vec<ElementSource>,
    /// Optional author intent for AI adapters. Core does not interpret it.
    #[serde(default)]
    pub ai: Option<AiSourcePolicy>,
}

/// Sandboxed include declaration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncludeSource {
    /// Logical resolver path.
    pub path: String,
    /// Optional namespace preventing ID collisions.
    #[serde(default)]
    pub namespace: Option<String>,
}

/// Component with typed/default props and element body.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentSource {
    /// Default prop values as frontend values.
    #[serde(default)]
    pub props: BTreeMap<String, serde_json::Value>,
    /// Named replaceable slots.
    #[serde(default)]
    pub slots: BTreeMap<String, Vec<ElementSource>>,
    /// Component body.
    #[serde(default)]
    pub elements: Vec<ElementSource>,
}

/// Theme tokens and optional parent.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeSource {
    /// Parent theme name.
    #[serde(default)]
    pub extends: Option<String>,
    /// Token values.
    #[serde(default)]
    pub tokens: BTreeMap<String, serde_json::Value>,
    /// Theme style layer.
    #[serde(default)]
    pub style: StyleSource,
}

/// Frontend style declaration. Typed conversion happens during expansion.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StyleSource {
    /// Fill color expression or token.
    pub fill: Option<ColorSource>,
    /// Stroke color expression or token.
    pub stroke: Option<ColorSource>,
    /// Stroke width.
    pub stroke_width: Option<Length>,
    /// Opacity in millionths.
    pub opacity: Option<u32>,
    /// Font family reference.
    pub font: Option<String>,
    /// Font size.
    pub font_size: Option<Length>,
    /// Text color expression or token.
    pub color: Option<ColorSource>,
}

/// String/token or explicit typed color accepted by the YAML frontend.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ColorSource {
    /// Named, hex, functional, or `$token` spelling.
    Text(String),
    /// Tagged `Color` value such as `{ space: cmyk, ... }`.
    Typed(Color),
}

/// Conditional style layer evaluated against the active binding context.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElementStyleRuleSource {
    /// Deterministic boolean expression.
    pub when: String,
    /// Partial style overlaid when the expression is truthy.
    pub style: StyleSource,
}

/// Named rectangular layout region.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionSource {
    /// Horizontal position.
    pub x: Length,
    /// Vertical position.
    pub y: Length,
    /// Width.
    pub width: Length,
    /// Height.
    pub height: Length,
    /// Optional inherited collision policy.
    #[serde(default)]
    pub collision: Option<CollisionSource>,
}

/// Supported typed data kinds.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataTypeSource {
    /// UTF-8 string.
    String,
    /// Signed integer.
    Integer,
    /// Exact decimal.
    Decimal,
    /// Boolean.
    Boolean,
    /// ISO date.
    Date,
    /// ISO date-time.
    DateTime,
    /// Duration.
    Duration,
    /// Exact decimal plus currency code object.
    Currency,
    /// Array.
    Array,
    /// Object.
    Object,
    /// Explicit null.
    Null,
}

/// Typed input field declaration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataFieldSource {
    /// Required type.
    #[serde(rename = "type")]
    pub data_type: DataTypeSource,
    /// Whether null is accepted.
    #[serde(default)]
    pub nullable: bool,
    /// Optional deterministic computed expression.
    #[serde(default)]
    pub computed: Option<String>,
}

/// Declarative element frontend.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElementSource {
    /// Unique stable ID.
    pub id: String,
    /// Element type.
    #[serde(rename = "type")]
    pub element_type: String,
    /// Optional component to instantiate.
    #[serde(default)]
    pub component: Option<String>,
    /// Component props.
    #[serde(default)]
    pub props: BTreeMap<String, serde_json::Value>,
    /// Named slot content supplied to a component instance.
    #[serde(default)]
    pub slots: BTreeMap<String, Vec<ElementSource>>,
    /// Horizontal source coordinate.
    #[serde(default)]
    pub x: Option<Length>,
    /// Vertical source coordinate.
    #[serde(default)]
    pub y: Option<Length>,
    /// Source width.
    #[serde(default)]
    pub width: Option<Length>,
    /// Source height.
    #[serde(default)]
    pub height: Option<Length>,
    /// Minimum, preferred, maximum, and aspect-ratio size intent.
    #[serde(default)]
    pub constraints: LayoutConstraints,
    /// Optional horizontal alignment inside the resolved container.
    #[serde(default)]
    pub align_x: Option<Alignment>,
    /// Optional vertical alignment inside the resolved container.
    #[serde(default)]
    pub align_y: Option<Alignment>,
    /// Literal text before binding evaluation.
    #[serde(default)]
    pub text: Option<String>,
    /// Text overflow, line, minimum-size, and writing-mode intent.
    #[serde(default)]
    pub text_options: TextSourceOptions,
    /// First-class table declaration, valid only for `type: table`.
    #[serde(default)]
    pub table: Option<TableSource>,
    /// Asset reference.
    #[serde(default)]
    pub asset: Option<String>,
    /// Image crop, fit, focal-point, and EXIF behavior.
    #[serde(default)]
    pub image: ImageOptions,
    /// Simple vector path commands.
    #[serde(default)]
    pub path: Vec<PathCommandSource>,
    /// Named style references in cascade order.
    #[serde(default)]
    pub styles: Vec<String>,
    /// Inline style.
    #[serde(default)]
    pub style: StyleSource,
    /// Ordered conditional styles evaluated after the compiled style layers.
    #[serde(default)]
    pub style_rules: Vec<ElementStyleRuleSource>,
    /// Translation, rotation, scale, flip, mirror, and origin intent.
    #[serde(default)]
    pub transform: TransformSource,
    /// Layout strategy.
    #[serde(default)]
    pub layout: LayoutMode,
    /// Distribution of children on a flow's primary axis.
    #[serde(default)]
    pub distribute: Distribution,
    /// Gap between flow children.
    #[serde(default = "default_gap")]
    pub gap: Length,
    /// Data binding expression.
    #[serde(default)]
    pub binding: Option<String>,
    /// Visibility condition expression.
    #[serde(default)]
    pub when: Option<String>,
    /// Repeat array expression.
    #[serde(default)]
    pub repeat: Option<String>,
    /// Named anchors.
    #[serde(default)]
    pub anchors: BTreeMap<String, String>,
    /// Named containing region.
    #[serde(default)]
    pub region: Option<String>,
    /// Child nodes.
    #[serde(default)]
    pub children: Vec<ElementSource>,
    /// Whether runtime patches may modify this node.
    #[serde(default)]
    pub locked: bool,
    /// Whether the node is initially hidden.
    #[serde(default)]
    pub hidden: bool,
    /// Visual layer.
    #[serde(default)]
    pub layer: String,
    /// Visual order within a layer.
    #[serde(default)]
    pub z_index: i32,
    /// Collision declaration independent from layer ordering.
    #[serde(default)]
    pub collision: Option<CollisionSource>,
    /// Expansion provenance populated by the compiler and absent from YAML.
    #[serde(skip)]
    #[doc(hidden)]
    pub provenance_components: Vec<String>,
    /// Logical include path populated by the compiler and absent from YAML.
    #[serde(skip)]
    #[doc(hidden)]
    pub provenance_source: Option<String>,
}

/// Geometry-first collision declaration or the shorthand `collision: false`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CollisionSource {
    /// Enables or disables collision using the default policy.
    Enabled(bool),
    /// Supplies the complete collision policy.
    Advanced(CollisionAdvancedSource),
}

impl Default for CollisionSource {
    fn default() -> Self {
        Self::Advanced(CollisionAdvancedSource::default())
    }
}

/// Advanced geometry-first collision declaration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CollisionAdvancedSource {
    /// Whether this node participates.
    pub enabled: bool,
    /// Collision group.
    pub group: String,
    /// Groups this node collides with; empty means all.
    pub collides_with: Vec<String>,
    /// Element IDs ignored by this node.
    pub ignore: Vec<String>,
    /// Higher values win movement conflicts.
    pub priority: i32,
    /// Whether the resolver may move this node.
    pub movable: bool,
    /// Resolved box used by the spatial index.
    pub bounds: CollisionBounds,
    /// Policy name: `push/error/overlay/next_page/shrink`.
    pub policy: String,
}

impl Default for CollisionAdvancedSource {
    fn default() -> Self {
        Self {
            enabled: true,
            group: "default".to_owned(),
            collides_with: Vec::new(),
            ignore: Vec::new(),
            priority: 0,
            movable: true,
            bounds: CollisionBounds::Layout,
            policy: "push".to_owned(),
        }
    }
}

/// Vector path source commands.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum PathCommandSource {
    /// Move current position.
    Move {
        /// Horizontal coordinate.
        x: Length,
        /// Vertical coordinate.
        y: Length,
    },
    /// Draw line.
    Line {
        /// Horizontal coordinate.
        x: Length,
        /// Vertical coordinate.
        y: Length,
    },
    /// Draw cubic Bézier curve.
    Curve {
        /// First control x.
        x1: Length,
        /// First control y.
        y1: Length,
        /// Second control x.
        x2: Length,
        /// Second control y.
        y2: Length,
        /// End x.
        x: Length,
        /// End y.
        y: Length,
    },
    /// Close current contour.
    Close,
}

/// Author-provided policy consumed only by the optional AI bridge.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiSourcePolicy {
    /// Compact statement of document purpose.
    #[serde(default)]
    pub purpose: String,
    /// Bounded textual edit rules.
    #[serde(default)]
    pub rules: Vec<String>,
    /// IDs the bridge may edit. Empty means policy default.
    #[serde(default)]
    pub editable: Vec<String>,
    /// IDs the bridge must never edit.
    #[serde(default)]
    pub locked: Vec<String>,
}
