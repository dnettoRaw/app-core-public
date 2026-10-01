// =============================================================================
//        #######
//     ###       ###     F: table_group_pagination.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/30 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/30 working-tree by dnettoRaw
//      ###########      S: 0.1.0-beta.5
// =============================================================================

//! Keeps bounded, contiguous table row groups together when they fit a page.
//!
//! Groups are buffered only within the dataset's declared row limit and are
//! measured against the physical page where they may be placed. A group that
//! fits a continuation page moves as a unit; an oversized group falls back to
//! row-at-a-time pagination, so it cannot create an empty-page retry loop.

use crate::table::{PaginationState, TablePaginator};
use crate::{
    DataRow, DataValue, Dataset, ErrorCode, FileMakerError, Result, TablePageSink, TableSpec, Unit,
};

struct MeasuredRow {
    row: DataRow,
    height: Unit,
    reserve_after: Unit,
    page_index: usize,
}

struct PendingGroup {
    key: DataValue,
    rows: Vec<MeasuredRow>,
}

pub(crate) fn paginate_measured(
    paginator: &TablePaginator,
    spec: &TableSpec,
    dataset: &dyn Dataset,
    measure: &mut dyn FnMut(usize, &DataRow) -> Result<Unit>,
    sink: &mut dyn TablePageSink,
    first_page_height: Unit,
    page_sensitive: bool,
) -> Result<()> {
    spec.validate()?;
    let mut state = PaginationState::default();
    let mut pending = None;
    spec.visit_bounded(dataset, &mut |_, row| {
        let key = group_key(spec, row)?;
        let Some(key) = key else {
            finish_pending(
                paginator,
                spec,
                &mut state,
                sink,
                &mut pending,
                measure,
                first_page_height,
                page_sensitive,
            )?;
            let measured = measure_row(spec, measure, state.page_index(), row)?;
            return push_row(
                paginator,
                spec,
                &mut state,
                sink,
                measured,
                measure,
                first_page_height,
                page_sensitive,
            );
        };
        if pending
            .as_ref()
            .is_some_and(|group: &PendingGroup| group.key != key)
        {
            finish_pending(
                paginator,
                spec,
                &mut state,
                sink,
                &mut pending,
                measure,
                first_page_height,
                page_sensitive,
            )?;
        }
        let page_index = state.page_index();
        let measured = measure_row(spec, measure, page_index, row)?;
        let group = pending.get_or_insert_with(|| PendingGroup {
            key: key.clone(),
            rows: Vec::new(),
        });
        group.rows.push(measured);
        Ok(())
    })?;

    finish_pending(
        paginator,
        spec,
        &mut state,
        sink,
        &mut pending,
        measure,
        first_page_height,
        page_sensitive,
    )?;
    if !state.is_empty() {
        state.flush(spec, sink, paginator.max_pages, true)?;
    }
    Ok(())
}

fn group_key(spec: &TableSpec, row: &DataRow) -> Result<Option<DataValue>> {
    spec.keep_together_by
        .as_ref()
        .map(|field| {
            row.get(field)
                .cloned()
                .ok_or_else(|| table_error("keep_together_by field is missing from a row"))
        })
        .transpose()
        .map(|value| value.filter(|value| !matches!(value, DataValue::Null)))
}

#[allow(clippy::too_many_arguments)]
fn finish_pending(
    paginator: &TablePaginator,
    spec: &TableSpec,
    state: &mut PaginationState,
    sink: &mut dyn TablePageSink,
    pending: &mut Option<PendingGroup>,
    measure: &mut dyn FnMut(usize, &DataRow) -> Result<Unit>,
    first_page_height: Unit,
    page_sensitive: bool,
) -> Result<()> {
    let Some(group) = pending.take() else {
        return Ok(());
    };
    let continuation_index = state.page_index().max(1);
    let continuation_rows = measure_rows(&group.rows, continuation_index, measure, page_sensitive)?;
    let continuation_height = rows_height(&continuation_rows)?;
    let continuation_capacity =
        paginator.content_height(spec, continuation_index, first_page_height)?;
    if continuation_height > continuation_capacity {
        for row in group.rows {
            push_row(
                paginator,
                spec,
                state,
                sink,
                row,
                measure,
                first_page_height,
                page_sensitive,
            )?;
        }
        return Ok(());
    }
    let mut measured = measure_rows(&group.rows, state.page_index(), measure, page_sensitive)?;
    let mut group_height = rows_height(&measured)?;
    let mut content_height =
        paginator.content_height(spec, state.page_index(), first_page_height)?;
    if !state.is_empty() && !state.can_fit(group_height, content_height)? {
        state.flush(spec, sink, paginator.max_pages, false)?;
        measured = measure_rows(&group.rows, state.page_index(), measure, page_sensitive)?;
        group_height = rows_height(&measured)?;
        content_height = paginator.content_height(spec, state.page_index(), first_page_height)?;
    }
    if group_height > content_height && state.is_empty() && state.page_index() == 0 {
        state.skip_empty_page()?;
        measured = measure_rows(&group.rows, state.page_index(), measure, page_sensitive)?;
        group_height = rows_height(&measured)?;
        content_height = paginator.content_height(spec, state.page_index(), first_page_height)?;
    }
    if group_height > content_height {
        return Err(table_error("kept row group does not fit its page"));
    }
    for measured in measured {
        state.push(spec, &measured.row, measured.height, measured.reserve_after)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn push_row(
    paginator: &TablePaginator,
    spec: &TableSpec,
    state: &mut PaginationState,
    sink: &mut dyn TablePageSink,
    mut measured: MeasuredRow,
    measure: &mut dyn FnMut(usize, &DataRow) -> Result<Unit>,
    first_page_height: Unit,
    page_sensitive: bool,
) -> Result<()> {
    let mut height = if page_sensitive && measured.page_index != state.page_index() {
        measured.page_index = state.page_index();
        measured.height = measure(state.page_index(), &measured.row)?;
        measured.height
    } else {
        measured.height
    };
    if height <= Unit::ZERO {
        return Err(table_error("measured table row height must be positive"));
    }
    let mut content_height =
        paginator.content_height(spec, state.page_index(), first_page_height)?;
    let mut required = height.checked_add(measured.reserve_after)?;
    if !state.is_empty() && !state.can_fit(required, content_height)? {
        state.flush(spec, sink, paginator.max_pages, false)?;
        if page_sensitive {
            measured.page_index = state.page_index();
            measured.height = measure(state.page_index(), &measured.row)?;
            height = measured.height;
            required = height.checked_add(measured.reserve_after)?;
        }
        content_height = paginator.content_height(spec, state.page_index(), first_page_height)?;
    }
    if required > content_height && state.is_empty() && state.page_index() == 0 {
        state.skip_empty_page()?;
        content_height = paginator.content_height(spec, state.page_index(), first_page_height)?;
        if page_sensitive {
            measured.page_index = state.page_index();
            measured.height = measure(state.page_index(), &measured.row)?;
            height = measured.height;
        }
        required = height.checked_add(measured.reserve_after)?;
    }
    if required > content_height {
        return Err(table_error(format!(
            "measured table row and anchor reserve {required:?} exceed page content height {content_height:?}"
        )));
    }
    state.push(spec, &measured.row, height, measured.reserve_after)
}

fn measure_rows(
    rows: &[MeasuredRow],
    page_index: usize,
    measure: &mut dyn FnMut(usize, &DataRow) -> Result<Unit>,
    page_sensitive: bool,
) -> Result<Vec<MeasuredRow>> {
    rows.iter()
        .map(|row| {
            let height = if page_sensitive && row.page_index != page_index {
                measure(page_index, &row.row)?
            } else {
                row.height
            };
            if height <= Unit::ZERO {
                return Err(table_error("measured table row height must be positive"));
            }
            Ok(MeasuredRow {
                row: row.row.clone(),
                height,
                reserve_after: row.reserve_after,
                page_index,
            })
        })
        .collect()
}

fn measure_row(
    spec: &TableSpec,
    measure: &mut dyn FnMut(usize, &DataRow) -> Result<Unit>,
    page_index: usize,
    row: &DataRow,
) -> Result<MeasuredRow> {
    let height = measure(page_index, row)?;
    if height <= Unit::ZERO {
        return Err(table_error("measured table row height must be positive"));
    }
    Ok(MeasuredRow {
        row: row.clone(),
        height,
        reserve_after: spec.reserve_after_for(row)?,
        page_index,
    })
}

fn rows_height(rows: &[MeasuredRow]) -> Result<Unit> {
    rows.iter().try_fold(Unit::ZERO, |total, row| {
        total
            .checked_add(row.height)?
            .checked_add(row.reserve_after)
    })
}

fn table_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::DataType, message)
}
