// =============================================================================
//        #######
//     ###       ###     F: layout_table_stream.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/31 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/31 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Converts each bounded table page immediately into resolved geometry.

use std::cell::RefCell;

use crate::layout_table::{build_cells, compute_row_style, measure_row, TableTextMeasurer};
use crate::{
    ComputedStyle, DataRow, ErrorCode, FileMakerError, Rect, ResolvedTableColumn,
    ResolvedTableFragment, ResolvedTableRow, Result, TablePage, TablePageSink, TableSpec, Unit,
};

pub(crate) struct FragmentCollector<'a, 'm> {
    columns: &'a [ResolvedTableColumn],
    base_style: &'a ComputedStyle,
    spec: &'a TableSpec,
    bounds: Rect,
    first_bounds: Rect,
    continuation_bounds: Rect,
    header_height: Unit,
    fixed_row_height: Option<Unit>,
    logical_unit: Unit,
    max_pages: usize,
    measurer: &'a RefCell<&'m mut dyn TableTextMeasurer>,
    source_index: u64,
    fragments: Vec<ResolvedTableFragment>,
}

impl<'a, 'm> FragmentCollector<'a, 'm> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        columns: &'a [ResolvedTableColumn],
        base_style: &'a ComputedStyle,
        spec: &'a TableSpec,
        bounds: Rect,
        first_bounds: Rect,
        continuation_bounds: Rect,
        header_height: Unit,
        fixed_row_height: Option<Unit>,
        logical_unit: Unit,
        max_pages: usize,
        measurer: &'a RefCell<&'m mut dyn TableTextMeasurer>,
    ) -> Self {
        Self {
            columns,
            base_style,
            spec,
            bounds,
            first_bounds,
            continuation_bounds,
            header_height,
            fixed_row_height,
            logical_unit,
            max_pages,
            measurer,
            source_index: 0,
            fragments: Vec::new(),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.fragments.is_empty()
    }

    pub(crate) fn finish(self) -> Vec<ResolvedTableFragment> {
        self.fragments
    }

    fn totals_height(&self, page: &TablePage) -> Result<Option<Unit>> {
        if page.totals.is_empty() {
            return Ok(None);
        }
        let mut measurer = self.measurer.borrow_mut();
        self.fixed_row_height
            .map_or_else(
                || {
                    measure_row(
                        &page.totals,
                        self.columns,
                        self.base_style,
                        self.spec,
                        super::layout_table::TableMeasureContext {
                            page_index: page.index,
                            bounds: self.bounds,
                            logical_unit: self.logical_unit,
                        },
                        &mut **measurer,
                    )
                },
                Ok,
            )
            .map(Some)
    }

    fn append(&mut self, page: TablePage, totals_height: Option<Unit>) -> Result<()> {
        let fragment = self.resolve_fragment(page, totals_height)?;
        self.fragments.push(fragment);
        Ok(())
    }

    fn resolve_fragment(
        &mut self,
        mut page: TablePage,
        totals_height: Option<Unit>,
    ) -> Result<ResolvedTableFragment> {
        if page.row_padding.is_empty() && !page.rows.is_empty() {
            page.row_padding = vec![crate::CellPadding::default(); page.rows.len()];
        }
        if page.rows.len() != page.row_heights.len()
            || page.rows.len() != page.row_styles.len()
            || page.rows.len() != page.row_padding.len()
            || page.rows.len() != page.group_starts.len()
        {
            return Err(table_stream_error(
                "table page row metadata lengths do not match its rows",
            ));
        }
        let mut measurer = self.measurer.borrow_mut();
        let bounds = if page.index == 0 {
            self.first_bounds
        } else {
            self.continuation_bounds
        };
        let mut y = bounds.origin.y;
        let header = if page.header {
            let cells = build_cells(
                self.columns,
                &DataRow::new(),
                self.base_style,
                bounds.origin.x,
                y,
                self.header_height,
                true,
                crate::Insets::default(),
                Unit::ZERO,
                &mut **measurer,
            )?;
            y = y.checked_add(self.header_height)?;
            cells
        } else {
            Vec::new()
        };
        let mut rows = Vec::with_capacity(page.rows.len());
        for ((((row, height), row_style), row_padding), group_start) in page
            .rows
            .into_iter()
            .zip(page.row_heights)
            .zip(page.row_styles)
            .zip(page.row_padding)
            .zip(page.group_starts)
        {
            let style = compute_row_style(self.base_style, &row_style)?;
            let row_padding = row_padding.resolve(self.logical_unit)?;
            let text_offset_y = self
                .spec
                .text_offset_y_for(&row, self.logical_unit, page.index)?;
            let row_bounds = Rect::new(bounds.origin.x, y, bounds.size.width, height)?;
            rows.push(ResolvedTableRow {
                source_index: self.source_index,
                bounds: row_bounds,
                group_start,
                style: style.clone(),
                cells: build_cells(
                    self.columns,
                    &row,
                    &style,
                    bounds.origin.x,
                    y,
                    height,
                    false,
                    row_padding,
                    text_offset_y,
                    &mut **measurer,
                )?,
            });
            self.source_index = self
                .source_index
                .checked_add(1)
                .ok_or_else(|| table_stream_error("table source row index overflow"))?;
            y = y.checked_add(height)?;
        }
        let totals = match totals_height {
            Some(height) => {
                let text_offset_y =
                    self.spec
                        .text_offset_y_for(&page.totals, self.logical_unit, page.index)?;
                build_cells(
                    self.columns,
                    &page.totals,
                    self.base_style,
                    bounds.origin.x,
                    y,
                    height,
                    false,
                    crate::Insets::default(),
                    text_offset_y,
                    &mut **measurer,
                )?
            }
            None => Vec::new(),
        };
        Ok(ResolvedTableFragment {
            index: page.index,
            columns: self.columns.to_vec(),
            header,
            rows,
            totals,
            starting_group: page.starting_group,
        })
    }
}

impl TablePageSink for FragmentCollector<'_, '_> {
    fn page(&mut self, mut page: TablePage) -> Result<()> {
        let Some(totals_height) = self.totals_height(&page)? else {
            return self.append(page, None);
        };
        let page_bounds = if page.index == 0 {
            self.first_bounds
        } else {
            self.continuation_bounds
        };
        let used = page
            .row_heights
            .iter()
            .try_fold(Unit::ZERO, |total, height| total.checked_add(*height))?
            .checked_add(page.rows.iter().try_fold(Unit::ZERO, |total, row| {
                total.checked_add(self.spec.reserve_after_for(row)?)
            })?)?
            .checked_add(if page.header {
                self.header_height
            } else {
                Unit::ZERO
            })?;
        if used.checked_add(totals_height)? <= page_bounds.size.height {
            return self.append(page, Some(totals_height));
        }
        let next_index = page
            .index
            .checked_add(1)
            .ok_or_else(|| table_stream_error("table page index overflow"))?;
        if next_index >= self.max_pages {
            return Err(FileMakerError::new(
                ErrorCode::LimitExceeded,
                "table totals require an additional page beyond the configured limit",
            ));
        }
        let totals = std::mem::take(&mut page.totals);
        self.append(page, None)?;
        self.append(
            TablePage {
                index: next_index,
                header: self.spec.repeat_header,
                rows: Vec::new(),
                row_heights: Vec::new(),
                row_styles: Vec::new(),
                row_padding: Vec::new(),
                group_starts: Vec::new(),
                starting_group: None,
                totals,
            },
            Some(totals_height),
        )
    }
}

fn table_stream_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::LayoutInvalid, message)
}
