// =============================================================================
//        #######
//     ###       ###     F: table.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded table contracts and behavior for this crate.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    DataValue, ErrorCode, Expression, ExpressionBudget, FileMakerError, Length, Result, Style,
};
#[path = "table_dataset.rs"]
mod dataset;
pub use dataset::{BorrowedDataset, DataRow, Dataset, InMemoryDataset, StreamingDataset};
#[path = "table_spec.rs"]
mod spec;

/// Table column sizing strategy.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "mode",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ColumnWidth {
    /// Exact width.
    Fixed(Length),
    /// Share of remaining width.
    Flex(u32),
    /// Width measured from bounded row samples.
    Auto,
}

/// Per-column cell insets, resolved against each column and table body.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CellPadding {
    /// Top inset.
    pub top: Length,
    /// Right inset.
    pub right: Length,
    /// Bottom inset.
    pub bottom: Length,
    /// Left inset.
    pub left: Length,
}

impl Default for CellPadding {
    fn default() -> Self {
        Self {
            top: Length::Absolute(crate::Unit::ZERO),
            right: Length::Absolute(crate::Unit::ZERO),
            bottom: Length::Absolute(crate::Unit::ZERO),
            left: Length::Absolute(crate::Unit::ZERO),
        }
    }
}

impl CellPadding {
    fn is_valid(self) -> bool {
        [self.top, self.right, self.bottom, self.left]
            .into_iter()
            .all(|length| match length {
                Length::Absolute(value) => value >= crate::Unit::ZERO,
                Length::Logical(value) => value >= 0,
                Length::Percent(_) | Length::Auto => false,
            })
    }

    /// Resolves absolute and caller-logical inset lengths.
    pub fn resolve(self, logical_unit: crate::Unit) -> Result<crate::Insets> {
        let resolve = |length| match length {
            Length::Absolute(value) if value >= crate::Unit::ZERO => Ok(value),
            Length::Logical(value) if value >= 0 => logical_unit.checked_scale(value),
            _ => Err(table_error(
                "cell padding must use nonnegative absolute or logical lengths",
            )),
        };
        Ok(crate::Insets {
            top: resolve(self.top)?,
            right: resolve(self.right)?,
            bottom: resolve(self.bottom)?,
            left: resolve(self.left)?,
        })
    }
}

/// One first-class table column.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableColumn {
    /// Stable field name.
    pub field: String,
    /// Header text.
    pub header: String,
    /// Sizing strategy.
    pub width: ColumnWidth,
    /// Inline alignment applied to every cell in this column.
    #[serde(default)]
    pub align_x: crate::Alignment,
    /// Per-side padding applied to every header, body, and totals cell.
    #[serde(default)]
    pub padding: CellPadding,
}

/// Conditional data-row style evaluated without IO or exporter state.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableStyleRule {
    /// Deterministic expression evaluated against the row object.
    pub when: String,
    /// Partial style applied when the expression is truthy.
    pub style: Style,
    /// Per-side cell padding applied to every cell in a matching row.
    #[serde(default)]
    pub padding: Option<CellPadding>,
    /// First-page cell padding for rows matching this rule.
    #[serde(default)]
    pub padding_first_page: Option<CellPadding>,
    /// Continuation-page cell padding for rows matching this rule.
    #[serde(default)]
    pub padding_continuation: Option<CellPadding>,
    /// Paint-only vertical translation for text in matching row cells.
    #[serde(default)]
    pub text_offset_y: Option<Length>,
    /// First-page paint-only vertical translation for matching row cells.
    #[serde(default)]
    pub text_offset_y_first_page: Option<Length>,
    /// Continuation-page paint-only vertical translation for matching row cells.
    #[serde(default)]
    pub text_offset_y_continuation: Option<Length>,
    /// Minimum measured height for matching rows.
    #[serde(default)]
    pub min_height: Option<Length>,
    /// Minimum row height on the first physical page.
    #[serde(default)]
    pub min_height_first_page: Option<Length>,
    /// Minimum row height on continuation pages.
    #[serde(default)]
    pub min_height_continuation: Option<Length>,
    /// Additional page capacity reserved after a matching anchored row.
    #[serde(default)]
    pub reserve_after: Option<Length>,
}

/// Pagination and grouping contract for a table.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TableSpec {
    /// Columns in visual order.
    pub columns: Vec<TableColumn>,
    /// Repeat header after pagination.
    pub repeat_header: bool,
    /// Optional grouping field.
    pub group_by: Option<String>,
    /// Optional row field whose contiguous equal values are kept on one page
    /// when the complete group fits; oversized groups split between rows.
    #[serde(default)]
    pub keep_together_by: Option<String>,
    /// Optional row field whose string values publish named row-bound anchors.
    #[serde(default)]
    pub row_anchor_field: Option<String>,
    /// Fields totaled as exact numeric values.
    pub total_fields: Vec<String>,
    /// Conditional row styles in stable declaration order.
    pub conditional_styles: Vec<TableStyleRule>,
    /// Per-row expression step budget shared by conditional rules.
    pub style_expression_steps: usize,
    /// Maximum rows sampled for automatic sizing.
    pub auto_sample_rows: usize,
    /// Maximum rows accepted from the dataset.
    pub max_rows: u64,
    /// Maximum fields accepted in one streamed row.
    pub max_row_fields: usize,
    /// Maximum displayed bytes accepted in one cell.
    pub max_cell_bytes: usize,
}

/// One bounded table page delivered to a sink.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TablePage {
    /// Zero-based table page index.
    pub index: usize,
    /// Whether the header must be rendered on this page.
    pub header: bool,
    /// Rows bounded by the computed per-page capacity.
    pub rows: Vec<DataRow>,
    /// Measured height corresponding one-to-one with `rows`.
    pub row_heights: Vec<crate::Unit>,
    /// Computed conditional style corresponding one-to-one with `rows`.
    pub row_styles: Vec<Style>,
    /// Additional per-side padding corresponding one-to-one with `rows`.
    #[serde(default)]
    pub row_padding: Vec<CellPadding>,
    /// Group key when each row begins a new group.
    pub group_starts: Vec<Option<String>>,
    /// Group key active at the first row, when configured.
    pub starting_group: Option<String>,
    /// Exact totals emitted only on the final table page.
    pub totals: BTreeMap<String, DataValue>,
}

/// Streaming page consumer.
pub trait TablePageSink {
    /// Accepts one complete bounded page.
    fn page(&mut self, page: TablePage) -> Result<()>;
}

/// Fixed-point bounded table pagination engine.
pub struct TablePaginator {
    /// Height available on each continuation page.
    pub available_height: crate::Unit,
    /// Header height.
    pub header_height: crate::Unit,
    /// Fixed row height after external text measurement.
    pub row_height: crate::Unit,
    /// Maximum generated pages.
    pub max_pages: usize,
}

impl TablePaginator {
    /// Streams paginated rows without retaining the entire dataset.
    pub fn paginate(
        &self,
        spec: &TableSpec,
        dataset: &dyn Dataset,
        sink: &mut dyn TablePageSink,
    ) -> Result<()> {
        if self.row_height <= crate::Unit::ZERO {
            return Err(table_error("fixed table row height must be positive"));
        }
        self.paginate_measured(spec, dataset, &mut |_| Ok(self.row_height), sink)
    }

    /// Streams rows using a deterministic externally measured height per row.
    pub fn paginate_measured(
        &self,
        spec: &TableSpec,
        dataset: &dyn Dataset,
        measure: &mut dyn FnMut(&DataRow) -> Result<crate::Unit>,
        sink: &mut dyn TablePageSink,
    ) -> Result<()> {
        self.paginate_measured_with_first_page_height(
            spec,
            dataset,
            measure,
            sink,
            self.available_height,
        )
    }

    pub(crate) fn paginate_measured_with_first_page_height(
        &self,
        spec: &TableSpec,
        dataset: &dyn Dataset,
        measure: &mut dyn FnMut(&DataRow) -> Result<crate::Unit>,
        sink: &mut dyn TablePageSink,
        first_page_height: crate::Unit,
    ) -> Result<()> {
        let mut contextual_measure = |_: usize, row: &DataRow| measure(row);
        self.paginate_measured_with_page_index(
            spec,
            dataset,
            &mut contextual_measure,
            sink,
            first_page_height,
            false,
        )
    }

    pub(crate) fn paginate_measured_with_page_index(
        &self,
        spec: &TableSpec,
        dataset: &dyn Dataset,
        measure: &mut dyn FnMut(usize, &DataRow) -> Result<crate::Unit>,
        sink: &mut dyn TablePageSink,
        first_page_height: crate::Unit,
        page_sensitive: bool,
    ) -> Result<()> {
        if self.available_height <= crate::Unit::ZERO
            || first_page_height <= crate::Unit::ZERO
            || self.header_height < crate::Unit::ZERO
            || self.max_pages == 0
        {
            return Err(table_error("table pagination dimensions are invalid"));
        }
        crate::table_group_pagination::paginate_measured(
            self,
            spec,
            dataset,
            measure,
            sink,
            first_page_height,
            page_sensitive,
        )
    }

    pub(crate) fn content_height(
        &self,
        spec: &TableSpec,
        page_index: usize,
        first_page_height: crate::Unit,
    ) -> Result<crate::Unit> {
        let available_height = if page_index == 0 {
            first_page_height
        } else {
            self.available_height
        };
        if page_index == 0 || spec.repeat_header {
            available_height.checked_sub(self.header_height)
        } else {
            Ok(available_height)
        }
    }
}

#[derive(Default)]
pub(crate) struct PaginationState {
    page_index: usize,
    rows: Vec<DataRow>,
    row_heights: Vec<crate::Unit>,
    row_styles: Vec<Style>,
    row_padding: Vec<CellPadding>,
    group_starts: Vec<Option<String>>,
    starting_group: Option<String>,
    last_group: Option<String>,
    used_height: crate::Unit,
    totals: BTreeMap<String, DataValue>,
}

impl PaginationState {
    pub(crate) fn push(
        &mut self,
        spec: &TableSpec,
        row: &DataRow,
        height: crate::Unit,
        reserve_after: crate::Unit,
    ) -> Result<()> {
        let group = spec
            .group_by
            .as_ref()
            .and_then(|field| row.get(field))
            .map(DataValue::display);
        if self.rows.is_empty() {
            self.starting_group.clone_from(&group);
        }
        self.group_starts
            .push((group != self.last_group).then(|| group.clone()).flatten());
        self.last_group = group;
        accumulate_totals(&mut self.totals, &spec.total_fields, row)?;
        self.row_styles.push(spec.style_for(row)?);
        self.row_padding
            .push(spec.padding_for(row, self.page_index)?);
        self.row_heights.push(height);
        self.rows.push(row.clone());
        self.used_height = self
            .used_height
            .checked_add(height)?
            .checked_add(reserve_after)?;
        Ok(())
    }

    pub(crate) fn flush(
        &mut self,
        spec: &TableSpec,
        sink: &mut dyn TablePageSink,
        max_pages: usize,
        final_page: bool,
    ) -> Result<()> {
        if self.page_index >= max_pages {
            return Err(FileMakerError::new(
                ErrorCode::LimitExceeded,
                "table page limit exceeded",
            ));
        }
        sink.page(TablePage {
            index: self.page_index,
            header: self.page_index == 0 || spec.repeat_header,
            rows: std::mem::take(&mut self.rows),
            row_heights: std::mem::take(&mut self.row_heights),
            row_styles: std::mem::take(&mut self.row_styles),
            row_padding: std::mem::take(&mut self.row_padding),
            group_starts: std::mem::take(&mut self.group_starts),
            starting_group: self.starting_group.take(),
            totals: if final_page {
                std::mem::take(&mut self.totals)
            } else {
                BTreeMap::new()
            },
        })?;
        self.page_index += 1;
        self.used_height = crate::Unit::ZERO;
        Ok(())
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub(crate) fn can_fit(&self, height: crate::Unit, capacity: crate::Unit) -> Result<bool> {
        Ok(self.used_height.checked_add(height)? <= capacity)
    }

    pub(crate) fn page_index(&self) -> usize {
        self.page_index
    }

    pub(crate) fn skip_empty_page(&mut self) -> Result<()> {
        if !self.rows.is_empty() {
            return Err(table_error("cannot defer a non-empty table page"));
        }
        self.page_index = self
            .page_index
            .checked_add(1)
            .ok_or_else(|| table_error("table page index overflow"))?;
        Ok(())
    }
}

fn accumulate_totals(
    totals: &mut BTreeMap<String, DataValue>,
    fields: &[String],
    row: &DataRow,
) -> Result<()> {
    for field in fields {
        let Some(value) = row.get(field) else {
            continue;
        };
        if matches!(value, DataValue::Null) {
            continue;
        }
        let next = add_total(totals.get(field), value)?;
        totals.insert(field.clone(), next);
    }
    Ok(())
}

fn add_total(current: Option<&DataValue>, value: &DataValue) -> Result<DataValue> {
    match (current, value) {
        (None, DataValue::Integer(value)) => Ok(DataValue::Integer(*value)),
        (None, DataValue::Decimal(value)) => Ok(DataValue::Decimal(*value)),
        (None, DataValue::Currency(value)) => Ok(DataValue::Currency(value.clone())),
        (Some(DataValue::Integer(left)), DataValue::Integer(right)) => left
            .checked_add(*right)
            .map(DataValue::Integer)
            .ok_or_else(|| table_error("integer table total overflow")),
        (Some(DataValue::Decimal(left)), DataValue::Decimal(right)) => left
            .checked_add(*right)
            .map(DataValue::Decimal)
            .ok_or_else(|| table_error("decimal table total overflow")),
        (Some(DataValue::Integer(left)), DataValue::Decimal(right)) => {
            rust_decimal::Decimal::from(*left)
                .checked_add(*right)
                .map(DataValue::Decimal)
                .ok_or_else(|| table_error("decimal table total overflow"))
        }
        (Some(DataValue::Decimal(left)), DataValue::Integer(right)) => left
            .checked_add(rust_decimal::Decimal::from(*right))
            .map(DataValue::Decimal)
            .ok_or_else(|| table_error("decimal table total overflow")),
        (Some(DataValue::Currency(left)), DataValue::Currency(right))
            if left.code == right.code =>
        {
            left.amount
                .checked_add(right.amount)
                .map(|amount| {
                    DataValue::Currency(crate::CurrencyValue {
                        code: left.code.clone(),
                        amount,
                    })
                })
                .ok_or_else(|| table_error("currency table total overflow"))
        }
        _ => Err(table_error(
            "table total field contains incompatible or non-numeric values",
        )),
    }
}

pub(super) fn table_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::DataType, message)
}
