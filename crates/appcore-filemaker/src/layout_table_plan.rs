// =============================================================================
//        #######
//     ###       ###     F: layout_table_plan.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/17 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/17 12:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

//! Shared table measurement and page planning; exporters consume its geometry.
use crate::layout_table::{
    measure_row, page_body_bounds, resolve_fixed, table_layout_error, TableMeasureContext,
    TableTextMeasurer,
};
use crate::layout_table_stream::FragmentCollector;
use crate::{
    resolve_table_columns, BorrowedDataset, ComputedStyle, Length, Rect, ResolvedTableFragment,
    ResourceLimits, Result, TablePage, TablePageSink, TablePaginator, Unit,
};
use std::cell::RefCell;

pub(crate) fn resolve_with_tail(
    table: &crate::TableIr,
    bounds: Rect,
    base_style: &ComputedStyle,
    limits: &ResourceLimits,
    logical_unit: Unit,
    measurer: &mut dyn TableTextMeasurer,
    adaptive: Option<&crate::layout_adaptive::AdaptiveRuntime>,
) -> Result<Vec<ResolvedTableFragment>> {
    let first_bounds = adaptive.map_or_else(
        || page_body_bounds(table, bounds, 0, logical_unit),
        |a| a.body(bounds, 0, false, logical_unit),
    )?;
    let continuation_bounds = adaptive.map_or_else(
        || page_body_bounds(table, bounds, 1, logical_unit),
        |a| a.body(bounds, 1, false, logical_unit),
    )?;
    let mut tail = adaptive.map(|a| a.tail(bounds, logical_unit)).transpose()?;
    let dataset = BorrowedDataset::new(&table.rows);
    let columns = resolve_table_columns(
        &table.spec,
        &dataset,
        bounds.size.width,
        logical_unit,
        &mut |text| measurer.inline_width(text, base_style, bounds.size),
    )?;
    validate_column_padding(&columns)?;
    let header_height = resolve_fixed(table.header_height, bounds.size.height, logical_unit)?;
    if let Some(tail) = &mut tail {
        tail.header_height = header_height;
        tail.repeat_header = table.spec.repeat_header;
    }
    let fixed_row_height = fixed_row_height(table, bounds, logical_unit)?;
    let measurer = RefCell::new(measurer);
    let mut fragments = FragmentCollector::new(
        &columns,
        base_style,
        &table.spec,
        bounds,
        first_bounds,
        continuation_bounds,
        header_height,
        fixed_row_height,
        logical_unit,
        limits.max_pages,
        &measurer,
    );
    TablePaginator {
        available_height: continuation_bounds.size.height,
        header_height,
        row_height: fixed_row_height.unwrap_or(Unit::ZERO),
        max_pages: limits.max_pages,
    }
    .paginate_with_tail(
        &table.spec,
        &dataset,
        &mut |page_index, row| {
            if let Some(tail) = &tail {
                tail.control
                    .checkpoint(crate::ProgressPhase::Layout, 0, None)?;
            }
            let mut measurer = measurer.borrow_mut();
            let measured = fixed_row_height.map_or_else(
                || {
                    measure_row(
                        row,
                        &columns,
                        base_style,
                        &table.spec,
                        TableMeasureContext {
                            page_index,
                            bounds,
                            logical_unit,
                        },
                        &mut **measurer,
                    )
                },
                Ok,
            )?;
            let minimum = table.spec.minimum_row_height_for(
                row,
                page_index,
                bounds.size.height,
                logical_unit,
            )?;
            Ok(minimum.map_or(measured, |minimum| measured.max(minimum)))
        },
        &mut fragments,
        first_bounds.size.height,
        true,
        tail.as_ref(),
    )?;
    if fragments.is_empty() {
        fragments.page(TablePage {
            index: 0,
            header: true,
            rows: Vec::new(),
            row_heights: Vec::new(),
            row_styles: Vec::new(),
            row_padding: Vec::new(),
            group_starts: Vec::new(),
            starting_group: None,
            totals: Default::default(),
        })?;
    }
    Ok(fragments.finish())
}

fn validate_column_padding(columns: &[crate::ResolvedTableColumn]) -> Result<()> {
    for resolved in columns {
        let horizontal = resolved.padding.left.checked_add(resolved.padding.right)?;
        if horizontal >= resolved.width {
            return Err(table_layout_error(
                "horizontal cell padding must leave positive content width",
            ));
        }
    }
    Ok(())
}

fn fixed_row_height(
    table: &crate::TableIr,
    bounds: Rect,
    logical_unit: Unit,
) -> Result<Option<Unit>> {
    table
        .row_height
        .filter(|height| !matches!(height, Length::Auto))
        .map(|height| resolve_fixed(height, bounds.size.height, logical_unit))
        .transpose()
}
