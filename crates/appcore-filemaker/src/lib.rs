// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Deterministic declarative document, canvas, and dataset compiler.

/// Explicit sandboxed asset and template resolvers.
pub mod asset;
mod binding_style;
/// Bounded compile-once/render-many caches.
pub mod cache;
/// Geometry-first collision policies and deterministic spatial index.
pub mod collision;
/// Parse, include expansion, component expansion, and binding pipeline.
pub mod compiler;
mod compiler_bind;
mod compiler_style;
/// Minimum, preferred, maximum, aspect, and alignment contracts.
pub mod constraints;
/// Cooperative cancellation and progress contracts.
pub mod control;
/// Typed deterministic data values and schemas.
pub mod data;
/// Read-only debug overlays and geometry-derived collision masks.
pub mod debug;
/// JSON/vector/raster export for geometry-derived debug masks.
pub mod debug_export;
mod debug_geometry;
mod debug_plan;
#[cfg(test)]
mod debug_tests;
mod diagnostic_budget;
/// Typed errors and stable FM codes.
pub mod error;
/// Capability-declared exporters selected only at call time.
pub mod export;
/// Bounded deterministic expression evaluation.
pub mod expression;
/// Deterministic input fingerprints.
pub mod fingerprint;
/// Explicit deterministic font registry.
pub mod font;
/// Fixed-point geometry contracts.
pub mod geometry;
/// Image crop, fit, focal point, EXIF, and resolved paint geometry.
pub mod image;
/// Read-only scene inspection and free-region queries.
pub mod inspect;
/// Format-neutral intermediate representations.
pub mod ir;
/// Measurement, layout, pagination, and bounded reflow engine.
pub mod layout;
mod layout_collision;
mod layout_context;
mod layout_exclusion;
mod layout_flow;
mod layout_geometry;
mod layout_measure;
mod layout_page;
mod layout_policy;
mod layout_region;
mod layout_table;
mod layout_table_stream;
#[cfg(test)]
mod layout_table_tests;
/// Resource budgets.
pub mod limits;
mod memory;
/// Page roles, masters, margins, bleed, safe areas, and numbering.
pub mod page;
/// Runtime mutation and transactional patch application.
pub mod patch;
mod patch_log;
/// Versioned page and canvas presets.
pub mod preset;
/// Fully resolved exporter-facing scene.
pub mod resolved;
mod resolved_table;
/// Strict version-one YAML frontend types.
pub mod source;
mod source_build;
mod source_element;
mod source_layout;
mod source_page;
mod source_page_build;
mod source_style;
mod source_table;
mod source_text;
mod source_transform;
/// Colors, themes, and style cascade contracts.
pub mod style;
/// First-class bounded and streaming datasets/tables.
pub mod table;
mod table_columns;
/// Unicode, `BiDi`, shaping, line breaking, and measurement.
pub mod text;
#[cfg(test)]
mod text_break_tests;
mod transform_math;
/// Fixed-point units and source lengths.
pub mod units;
/// Schema, binding, layout, and exporter-aware preflight.
pub mod validation;
mod validation_capability;
mod validation_data;
mod validation_layout;
mod validation_page;
mod validation_table;

pub use asset::{Asset, AssetResolver, FileResolver, MemoryResolver, TemplateResolver};
pub use cache::SceneCache;
pub use collision::{
    CollisionBounds, CollisionPolicy, CollisionResolution, CollisionRule, LinearSpatialIndex,
    SpatialIndex,
};
pub use compiler::{Compiler, CompilerBuilder};
pub use constraints::{Alignment, Distribution, LayoutConstraints};
pub use control::{
    CancellationToken, OperationControl, ProgressEvent, ProgressObserver, ProgressPhase,
};
pub use data::{
    resolve_computed_fields, CurrencyValue, DataField, DataSchema, DataType, DataValue,
};
pub use debug::{CollisionMask, DebugOverlay, DebugOverlayOptions, DebugPrimitive, MaskView};
pub use debug_export::{export_collision_mask, MaskFormat};
pub use error::{ErrorCode, FileMakerError, Result};
pub use export::{
    export, export_bytes, export_controlled, export_dataset_csv, export_dataset_csv_bytes,
    ExportCapabilities, ExportContext, ExportFormat, ExportLoss, ExportLossKind, ExportLossReport,
    ExportOutcome, ExportRequest, ExportStyleOverride, Fidelity, HtmlMode, PdfMode,
};
pub use expression::{Expression, ExpressionBudget};
pub use fingerprint::{DocumentFingerprint, FingerprintBuilder};
pub use font::{FontAsset, FontManager, FontResolver, FontSubset};
pub use geometry::{BoundsSet, Insets, PathCommand, Point, Rect, Shape, Size, Transform};
pub use image::{
    resolve_image_placement, ImageCrop, ImageFit, ImageOptions, ImageOrientation, ImagePlacement,
    PixelRect,
};
pub use inspect::{ElementInspection, LayoutExplanation, PageInspection, SceneInspector};
pub use ir::{
    AiPolicy, DocumentIr, ElementId, ElementIr, ElementKind, ExclusionIr, GeometryIr, LayoutMode,
    PathCommandIr, Provenance, RegionIr, TableIr, TemplateIr, TextIr, TransformIr,
};
pub use layout::{LayoutEngine, LayoutOptions};
pub use limits::ResourceLimits;
pub use page::{PageBand, PagePlacement, PageRole, PageTemplate, PageTemplateSet};
pub use patch::{Patch, PatchOperation, PatchTransaction};
pub use patch_log::OperationLog;
pub use preset::{Orientation, Preset, PresetRegistry};
pub use resolved::{
    LayoutTrace, ResolvedElement, ResolvedExclusion, ResolvedPage, ResolvedRegion, ResolvedScene,
};
pub use resolved_table::{ResolvedTableCell, ResolvedTableFragment, ResolvedTableRow};
pub use source::{
    ColorSource, EdgeSource, ElementSource, ElementStyleRuleSource, ExclusionSource, MirrorSource,
    ModelKind, PageLayerSource, PageSource, TableSource, TableStyleRuleSource, TemplateSourceV1,
    TextSourceOptions, TransformSource,
};
pub use style::{Color, ComputedStyle, ElementStyleRule, Style, StyleCascade};
pub use table::{
    BorrowedDataset, ColumnWidth, DataRow, Dataset, InMemoryDataset, StreamingDataset, TableColumn,
    TablePage, TablePageSink, TablePaginator, TableSpec, TableStyleRule,
};
pub use table_columns::{resolve_table_columns, ResolvedTableColumn};
pub use text::{
    Glyph, GlyphRun, TextDiagnostic, TextEngine, TextLayout, TextLine, TextOptions, TextOverflow,
    WritingMode,
};
pub use units::{Length, Unit};
pub use validation::{
    preflight, validate_data, validate_layout, validate_template, PreflightOptions, ValidationCode,
    ValidationIssue, ValidationReport, ValidationSeverity,
};

/// Source schema accepted by this engine release.
pub const FILEMAKER_SCHEMA_V1: &str = "1.0";

/// Engine version included in deterministic fingerprints.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
