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

/// One deterministically ordered tabular row.
pub type DataRow = BTreeMap<String, DataValue>;

/// Restartable bounded dataset contract.
pub trait Dataset: Send + Sync {
    /// Optional exact row count.
    fn row_count_hint(&self) -> Option<u64>;
    /// Visits rows in stable order until the visitor returns `false`.
    fn visit_rows_until(
        &self,
        visitor: &mut dyn FnMut(u64, &DataRow) -> Result<bool>,
    ) -> Result<()>;

    /// Visits every row without requiring full materialization.
    fn visit_rows(&self, visitor: &mut dyn FnMut(u64, &DataRow) -> Result<()>) -> Result<()> {
        self.visit_rows_until(&mut |index, row| {
            visitor(index, row)?;
            Ok(true)
        })
    }
}

/// In-memory dataset for small bounded inputs.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct InMemoryDataset {
    /// Rows in source order.
    pub rows: Vec<DataRow>,
}

/// Borrowed dataset avoiding a duplicate row allocation for existing slices.
pub struct BorrowedDataset<'a> {
    rows: &'a [DataRow],
}

impl<'a> BorrowedDataset<'a> {
    /// Borrows rows in their existing stable source order.
    #[must_use]
    pub const fn new(rows: &'a [DataRow]) -> Self {
        Self { rows }
    }
}

impl Dataset for BorrowedDataset<'_> {
    fn row_count_hint(&self) -> Option<u64> {
        u64::try_from(self.rows.len()).ok()
    }

    fn visit_rows_until(
        &self,
        visitor: &mut dyn FnMut(u64, &DataRow) -> Result<bool>,
    ) -> Result<()> {
        visit_slice(self.rows, visitor)
    }
}

impl Dataset for InMemoryDataset {
    fn row_count_hint(&self) -> Option<u64> {
        u64::try_from(self.rows.len()).ok()
    }

    fn visit_rows_until(
        &self,
        visitor: &mut dyn FnMut(u64, &DataRow) -> Result<bool>,
    ) -> Result<()> {
        visit_slice(&self.rows, visitor)
    }
}

fn visit_slice(
    rows: &[DataRow],
    visitor: &mut dyn FnMut(u64, &DataRow) -> Result<bool>,
) -> Result<()> {
    for (index, row) in rows.iter().enumerate() {
        let index = u64::try_from(index).map_err(|_| table_error("row index overflow"))?;
        if !visitor(index, row)? {
            break;
        }
    }
    Ok(())
}

/// Factory-backed dataset enabling restartable streaming.
pub struct StreamingDataset<F> {
    factory: F,
    row_count_hint: Option<u64>,
}

impl<F> StreamingDataset<F> {
    /// Creates a dataset from a factory returning a fresh iterator per visit.
    #[must_use]
    pub const fn new(factory: F, row_count_hint: Option<u64>) -> Self {
        Self {
            factory,
            row_count_hint,
        }
    }
}

impl<F, I> Dataset for StreamingDataset<F>
where
    F: Fn() -> I + Send + Sync,
    I: Iterator<Item = Result<DataRow>>,
{
    fn row_count_hint(&self) -> Option<u64> {
        self.row_count_hint
    }

    fn visit_rows_until(
        &self,
        visitor: &mut dyn FnMut(u64, &DataRow) -> Result<bool>,
    ) -> Result<()> {
        for (index, row) in (self.factory)().enumerate() {
            let row = row?;
            if !visitor(
                u64::try_from(index).map_err(|_| table_error("row index overflow"))?,
                &row,
            )? {
                break;
            }
        }
        Ok(())
    }
}

/// Table column sizing strategy.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", content = "value", rename_all = "snake_case")]
pub enum ColumnWidth {
    /// Exact width.
    Fixed(Length),
    /// Share of remaining width.
    Flex(u32),
    /// Width measured from bounded row samples.
    Auto,
}

/// One first-class table column.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TableColumn {
    /// Stable field name.
    pub field: String,
    /// Header text.
    pub header: String,
    /// Sizing strategy.
    pub width: ColumnWidth,
}

/// Conditional data-row style evaluated without IO or exporter state.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableStyleRule {
    /// Deterministic expression evaluated against the row object.
    pub when: String,
    /// Partial style applied when the expression is truthy.
    pub style: Style,
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
        spec.validate()?;
        if self.available_height <= crate::Unit::ZERO
            || self.header_height < crate::Unit::ZERO
            || self.max_pages == 0
        {
            return Err(table_error("table pagination dimensions are invalid"));
        }
        let mut state = PaginationState::default();
        spec.visit_bounded(dataset, &mut |_, row| {
            let height = measure(row)?;
            let mut content_height = self.content_height(spec, state.page_index)?;
            if height <= crate::Unit::ZERO || height > content_height {
                return Err(table_error("measured table row does not fit on a page"));
            }
            if !state.rows.is_empty() && state.used_height.checked_add(height)? > content_height {
                state.flush(spec, sink, self.max_pages, false)?;
                content_height = self.content_height(spec, state.page_index)?;
                if height > content_height {
                    return Err(table_error("measured table row does not fit on a page"));
                }
            }
            state.push(spec, row, height)
        })?;
        if !state.rows.is_empty() {
            state.flush(spec, sink, self.max_pages, true)?;
        }
        Ok(())
    }

    fn content_height(&self, spec: &TableSpec, page_index: usize) -> Result<crate::Unit> {
        if page_index == 0 || spec.repeat_header {
            self.available_height.checked_sub(self.header_height)
        } else {
            Ok(self.available_height)
        }
    }
}

#[derive(Default)]
struct PaginationState {
    page_index: usize,
    rows: Vec<DataRow>,
    row_heights: Vec<crate::Unit>,
    row_styles: Vec<Style>,
    group_starts: Vec<Option<String>>,
    starting_group: Option<String>,
    last_group: Option<String>,
    used_height: crate::Unit,
    totals: BTreeMap<String, DataValue>,
}

impl PaginationState {
    fn push(&mut self, spec: &TableSpec, row: &DataRow, height: crate::Unit) -> Result<()> {
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
        self.row_heights.push(height);
        self.rows.push(row.clone());
        self.used_height = self.used_height.checked_add(height)?;
        Ok(())
    }

    fn flush(
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

impl TableSpec {
    /// Validates bounded table structure.
    pub fn validate(&self) -> Result<()> {
        let fields: std::collections::BTreeSet<_> =
            self.columns.iter().map(|column| &column.field).collect();
        let total_fields: std::collections::BTreeSet<_> = self.total_fields.iter().collect();
        if self.columns.is_empty()
            || self.columns.len() > 1_024
            || self.auto_sample_rows == 0
            || self.max_rows == 0
            || self.style_expression_steps == 0
            || self.max_row_fields == 0
            || self.max_cell_bytes == 0
            || self.columns.len() > self.max_row_fields
            || self.columns.iter().any(|column| column.field.is_empty())
            || fields.len() != self.columns.len()
            || self
                .group_by
                .as_ref()
                .is_some_and(|field| !fields.contains(field))
            || self
                .total_fields
                .iter()
                .any(|field| !fields.contains(field))
            || total_fields.len() != self.total_fields.len()
            || self.conditional_styles.len() > 1_024
            || self
                .conditional_styles
                .iter()
                .any(|rule| rule.when.is_empty())
            || self
                .columns
                .iter()
                .any(|column| matches!(column.width, ColumnWidth::Flex(0)))
        {
            return Err(table_error("table specification is invalid"));
        }
        for rule in &self.conditional_styles {
            Expression::parse(&rule.when)?;
            rule.style.validate()?;
        }
        Ok(())
    }

    /// Computes the ordered conditional style layers for one row.
    pub fn style_for(&self, row: &DataRow) -> Result<Style> {
        let root = DataValue::Object(row.clone());
        let mut computed = Style::default();
        let mut budget = ExpressionBudget::new(self.style_expression_steps)?;
        for rule in &self.conditional_styles {
            if Expression::parse(&rule.when)?
                .evaluate(&root, &mut budget)?
                .is_truthy()
            {
                rule.style.validate()?;
                computed.overlay(&rule.style);
            }
        }
        Ok(computed)
    }

    /// Streams rows with an enforced hard maximum.
    pub fn visit_bounded(
        &self,
        dataset: &dyn Dataset,
        visitor: &mut dyn FnMut(u64, &DataRow) -> Result<()>,
    ) -> Result<()> {
        self.visit_bounded_until(dataset, &mut |index, row| {
            visitor(index, row)?;
            Ok(true)
        })
    }

    /// Streams rows until a bounded visitor asks to stop.
    pub fn visit_bounded_until(
        &self,
        dataset: &dyn Dataset,
        visitor: &mut dyn FnMut(u64, &DataRow) -> Result<bool>,
    ) -> Result<()> {
        self.validate()?;
        dataset.visit_rows_until(&mut |index, row| {
            if index >= self.max_rows {
                return Err(FileMakerError::new(
                    ErrorCode::LimitExceeded,
                    "dataset row limit exceeded",
                ));
            }
            if row.len() > self.max_row_fields
                || row.iter().any(|(field, value)| {
                    field.len() > self.max_cell_bytes
                        || value.display().len() > self.max_cell_bytes
                        || matches!(value, DataValue::Array(_) | DataValue::Object(_))
                })
            {
                return Err(FileMakerError::new(
                    ErrorCode::LimitExceeded,
                    "dataset row exceeds its field, cell, or scalar-value limit",
                ));
            }
            visitor(index, row)
        })
    }
}

fn table_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::DataType, message)
}
