// =============================================================================
//        #######
//     ###       ###     F: source_table.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded source table contracts and behavior for this crate.

use serde::{Deserialize, Serialize};

use crate::source::StyleSource;
use crate::source_style::convert_style;
use crate::{
    ErrorCode, FileMakerError, Length, ResourceLimits, Result, TableColumn, TableIr, TableSpec,
    TableStyleRule,
};

/// Conditional table-row style in frontend color/length syntax.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableStyleRuleSource {
    /// Deterministic expression evaluated against one row.
    pub when: String,
    /// Partial data-rule style.
    #[serde(default)]
    pub style: StyleSource,
    /// Per-side cell padding applied to every cell in a matching row.
    #[serde(default)]
    pub padding: Option<crate::CellPadding>,
    /// First-page cell padding for rows matching this rule.
    #[serde(default)]
    pub padding_first_page: Option<crate::CellPadding>,
    /// Continuation-page cell padding for rows matching this rule.
    #[serde(default)]
    pub padding_continuation: Option<crate::CellPadding>,
    /// Paint-only vertical translation for text in matching row cells.
    #[serde(default)]
    pub text_offset_y: Option<Length>,
    /// First-page paint-only vertical translation for matching row cells.
    #[serde(default)]
    pub text_offset_y_first_page: Option<Length>,
    /// Continuation-page paint-only vertical translation for matching row cells.
    #[serde(default)]
    pub text_offset_y_continuation: Option<Length>,
    /// Minimum measured height for rows matching this rule.
    #[serde(default)]
    pub min_height: Option<Length>,
    /// First-page minimum height for matching rows.
    #[serde(default)]
    pub min_height_first_page: Option<Length>,
    /// Continuation-page minimum height for matching rows.
    #[serde(default)]
    pub min_height_continuation: Option<Length>,
    /// Additional page capacity reserved after a matching anchored row.
    #[serde(default)]
    pub reserve_after: Option<Length>,
}

/// Declarative first-class table options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableSource {
    /// Columns in stable visual order.
    pub columns: Vec<TableColumn>,
    /// Repeat the header on continuation pages.
    #[serde(default = "default_true")]
    pub repeat_header: bool,
    /// Optional grouping field.
    #[serde(default)]
    pub group_by: Option<String>,
    /// Optional row field that keeps contiguous equal values together when
    /// they fit one page; oversized groups split between complete rows.
    #[serde(default)]
    pub keep_together_by: Option<String>,
    /// Optional data field whose string values publish named row anchors.
    #[serde(default)]
    pub row_anchor_field: Option<String>,
    /// Exact numeric total fields.
    #[serde(default)]
    pub total_fields: Vec<String>,
    /// Ordered conditional row styles.
    #[serde(default)]
    pub conditional_styles: Vec<TableStyleRuleSource>,
    /// Maximum rows sampled for auto columns.
    #[serde(default = "default_auto_samples")]
    pub auto_sample_rows: usize,
    /// Optional stricter row limit.
    #[serde(default)]
    pub max_rows: Option<u64>,
    /// Optional stricter field-count limit per row.
    #[serde(default)]
    pub max_row_fields: Option<usize>,
    /// Optional stricter displayed byte limit per cell.
    #[serde(default)]
    pub max_cell_bytes: Option<usize>,
    /// Header height.
    #[serde(default = "default_header_height")]
    pub header_height: Length,
    /// Fixed row height or `auto` for measured rows.
    #[serde(default)]
    pub row_height: Option<Length>,
    /// Optional first-page and continuation body regions inside the table bounds.
    #[serde(default)]
    pub page_bodies: Option<crate::TablePageBodies>,
}

pub(crate) fn convert_table(source: &TableSource, limits: &ResourceLimits) -> Result<TableIr> {
    if matches!(source.header_height, Length::Auto) {
        return Err(table_source_error("table header height cannot be auto"));
    }
    if source.page_bodies.as_ref().is_some_and(|regions| {
        [
            &regions.first.offset_y,
            &regions.first.height,
            &regions.continuation.offset_y,
            &regions.continuation.height,
        ]
        .into_iter()
        .any(|length| matches!(length, Length::Auto))
    }) {
        return Err(table_source_error(
            "table page body offsets and heights cannot be auto",
        ));
    }
    reject_raised_limit(source.max_rows, limits.max_rows, "table row")?;
    reject_raised_limit(
        source.max_row_fields,
        limits.max_elements,
        "table row field",
    )?;
    reject_raised_limit(
        source.max_cell_bytes,
        limits.max_text_bytes,
        "table cell byte",
    )?;
    let spec = TableSpec {
        columns: source.columns.clone(),
        repeat_header: source.repeat_header,
        group_by: source.group_by.clone(),
        keep_together_by: source.keep_together_by.clone(),
        row_anchor_field: source.row_anchor_field.clone(),
        total_fields: source.total_fields.clone(),
        conditional_styles: source
            .conditional_styles
            .iter()
            .map(|rule| {
                Ok(TableStyleRule {
                    when: rule.when.clone(),
                    style: convert_style(&rule.style)?,
                    padding: rule.padding,
                    padding_first_page: rule.padding_first_page,
                    padding_continuation: rule.padding_continuation,
                    text_offset_y: rule.text_offset_y,
                    text_offset_y_first_page: rule.text_offset_y_first_page,
                    text_offset_y_continuation: rule.text_offset_y_continuation,
                    min_height: rule.min_height,
                    min_height_first_page: rule.min_height_first_page,
                    min_height_continuation: rule.min_height_continuation,
                    reserve_after: rule.reserve_after,
                })
            })
            .collect::<Result<Vec<_>>>()?,
        style_expression_steps: limits.max_expression_steps,
        auto_sample_rows: source.auto_sample_rows,
        max_rows: source.max_rows.unwrap_or(limits.max_rows),
        max_row_fields: source.max_row_fields.unwrap_or(limits.max_elements),
        max_cell_bytes: source.max_cell_bytes.unwrap_or(limits.max_text_bytes),
    };
    spec.validate()?;
    Ok(TableIr {
        spec,
        header_height: source.header_height,
        row_height: source.row_height,
        page_bodies: source.page_bodies.clone(),
        rows: Vec::new(),
    })
}

const fn default_true() -> bool {
    true
}

const fn default_auto_samples() -> usize {
    16
}

fn default_header_height() -> Length {
    Length::Absolute(crate::Unit::from_raw(18_000_000))
}

fn table_source_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::SchemaField, message)
}

fn reject_raised_limit<T>(requested: Option<T>, global: T, label: &str) -> Result<()>
where
    T: Copy + PartialOrd,
{
    if requested.is_some_and(|value| value > global) {
        return Err(FileMakerError::new(
            ErrorCode::LimitExceeded,
            format!("{label} limit cannot exceed the compiler limit"),
        ));
    }
    Ok(())
}
