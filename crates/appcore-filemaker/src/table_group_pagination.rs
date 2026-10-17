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

#[path = "table_group_diagnostics.rs"]
mod diagnostics;
use crate::table::{PaginationState, TablePaginator};
use crate::table_tail::TailReservation;
use crate::{
    DataRow, DataValue, Dataset, ErrorCode, FileMakerError, Result, TablePageSink, TableSpec, Unit,
};
use diagnostics::record;

struct MeasuredRow {
    source_index: u64,
    row: DataRow,
    height: Unit,
    reserve_after: Unit,
    page_index: usize,
    final_row: bool,
}

struct PendingGroup {
    key: Option<DataValue>,
    rows: Vec<MeasuredRow>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn paginate_measured(
    paginator: &TablePaginator,
    spec: &TableSpec,
    dataset: &dyn Dataset,
    measure: &mut dyn FnMut(usize, &DataRow) -> Result<Unit>,
    sink: &mut dyn TablePageSink,
    first_page_height: Unit,
    page_sensitive: bool,
    tail: Option<&TailReservation>,
) -> Result<()> {
    spec.validate()?;
    let mut state = PaginationState::default();
    let mut pending = None;
    spec.visit_bounded(dataset, &mut |index, row| {
        if let Some(tail) = tail {
            tail.control
                .checkpoint(crate::ProgressPhase::Layout, index, None)?;
        }
        let key = group_key(spec, row)?;
        if key.is_none() && tail.is_none() {
            finish_pending(
                paginator,
                spec,
                &mut state,
                sink,
                &mut pending,
                measure,
                first_page_height,
                page_sensitive,
                tail,
                false,
            )?;
            let measured = measure_row(spec, measure, state.page_index(), index, row)?;
            return push_row(
                paginator,
                spec,
                &mut state,
                sink,
                measured,
                measure,
                first_page_height,
                page_sensitive,
                tail,
            );
        };
        if pending
            .as_ref()
            .is_some_and(|group: &PendingGroup| key.is_none() || group.key != key)
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
                tail,
                false,
            )?;
        }
        let page_index = state.page_index();
        let measured = measure_row(spec, measure, page_index, index, row)?;
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
        tail,
        true,
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
    tail: Option<&TailReservation>,
    final_group: bool,
) -> Result<()> {
    let Some(mut group) = pending.take() else {
        return Ok(());
    };
    if final_group && tail.is_some() {
        if let Some(last) = group.rows.last_mut() {
            last.final_row = true;
        }
    }
    if tail.is_some() {
        let current = measure_rows(&group.rows, state.page_index(), measure, page_sensitive)?;
        let current_capacity =
            paginator.content_height(spec, state.page_index(), first_page_height)?;
        if fit_current(spec, state, tail, current, current_capacity)? {
            return Ok(());
        }
    }
    let continuation_index = state.page_index().max(1);
    let continuation_rows = measure_rows(&group.rows, continuation_index, measure, page_sensitive)?;
    let continuation_height = rows_height(&continuation_rows, continuation_index, tail)?;
    let continuation_capacity =
        paginator.content_height(spec, continuation_index, first_page_height)?;
    if continuation_height > continuation_capacity {
        if diagnostics::oversize(
            tail,
            continuation_index,
            &continuation_rows,
            continuation_capacity,
        )? {
            return Err(table_error(
                "row group and final reservation exceed page capacity; oversized_group is error",
            ));
        }
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
                tail,
            )?;
        }
        return Ok(());
    }
    let mut measured = measure_rows(&group.rows, state.page_index(), measure, page_sensitive)?;
    let mut group_height = rows_height(&measured, state.page_index(), tail)?;
    let mut content_height =
        paginator.content_height(spec, state.page_index(), first_page_height)?;
    if !state.is_empty() && !state.can_fit(group_height, content_height)? {
        record(
            tail,
            state,
            &measured,
            content_height,
            crate::PaginationRule::MoveGroup,
        )?;
        state.flush(spec, sink, paginator.max_pages, false)?;
        measured = measure_rows(&group.rows, state.page_index(), measure, page_sensitive)?;
        group_height = rows_height(&measured, state.page_index(), tail)?;
        content_height = paginator.content_height(spec, state.page_index(), first_page_height)?;
    }
    if group_height > content_height && state.is_empty() && state.page_index() == 0 {
        record(
            tail,
            state,
            &measured,
            content_height,
            crate::PaginationRule::SkipFirstPage,
        )?;
        state.skip_empty_page()?;
        measured = measure_rows(&group.rows, state.page_index(), measure, page_sensitive)?;
        group_height = rows_height(&measured, state.page_index(), tail)?;
        content_height = paginator.content_height(spec, state.page_index(), first_page_height)?;
    }
    if group_height > content_height {
        record(
            tail,
            state,
            &measured,
            content_height,
            crate::PaginationRule::Reject,
        )?;
        return Err(table_error("kept row group does not fit its page"));
    }
    record(
        tail,
        state,
        &measured,
        content_height,
        crate::PaginationRule::Fit,
    )?;
    for measured in measured {
        let reserve = reserve_at(&measured, state.page_index(), tail)?;
        state.push(spec, &measured.row, measured.height, reserve)?;
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
    tail: Option<&TailReservation>,
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
    let mut required = height.checked_add(reserve_at(&measured, state.page_index(), tail)?)?;
    if !state.is_empty() && !state.can_fit(required, content_height)? {
        record(
            tail,
            state,
            std::slice::from_ref(&measured),
            content_height,
            crate::PaginationRule::MoveRow,
        )?;
        state.flush(spec, sink, paginator.max_pages, false)?;
        if page_sensitive {
            measured.page_index = state.page_index();
            measured.height = measure(state.page_index(), &measured.row)?;
            height = measured.height;
            required = height.checked_add(reserve_at(&measured, state.page_index(), tail)?)?;
        }
        content_height = paginator.content_height(spec, state.page_index(), first_page_height)?;
    }
    if required > content_height && state.is_empty() && state.page_index() == 0 {
        record(
            tail,
            state,
            std::slice::from_ref(&measured),
            content_height,
            crate::PaginationRule::SkipFirstPage,
        )?;
        state.skip_empty_page()?;
        content_height = paginator.content_height(spec, state.page_index(), first_page_height)?;
        if page_sensitive {
            measured.page_index = state.page_index();
            measured.height = measure(state.page_index(), &measured.row)?;
            height = measured.height;
        }
        required = height.checked_add(reserve_at(&measured, state.page_index(), tail)?)?;
    }
    if required > content_height {
        record(
            tail,
            state,
            std::slice::from_ref(&measured),
            content_height,
            crate::PaginationRule::Reject,
        )?;
        return Err(table_error(format!(
            "measured table row and anchor reserve {required:?} exceed page content height {content_height:?}"
        )));
    }
    record(
        tail,
        state,
        std::slice::from_ref(&measured),
        content_height,
        crate::PaginationRule::Fit,
    )?;
    state.push(
        spec,
        &measured.row,
        height,
        reserve_at(&measured, state.page_index(), tail)?,
    )
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
                source_index: row.source_index,
                row: row.row.clone(),
                height,
                reserve_after: row.reserve_after,
                page_index,
                final_row: row.final_row,
            })
        })
        .collect()
}

fn measure_row(
    spec: &TableSpec,
    measure: &mut dyn FnMut(usize, &DataRow) -> Result<Unit>,
    page_index: usize,
    source_index: u64,
    row: &DataRow,
) -> Result<MeasuredRow> {
    let height = measure(page_index, row)?;
    if height <= Unit::ZERO {
        return Err(table_error("measured table row height must be positive"));
    }
    Ok(MeasuredRow {
        source_index,
        row: row.clone(),
        height,
        reserve_after: spec.reserve_after_for(row)?,
        page_index,
        final_row: false,
    })
}

fn rows_height(rows: &[MeasuredRow], page: usize, tail: Option<&TailReservation>) -> Result<Unit> {
    rows.iter().try_fold(Unit::ZERO, |total, row| {
        total
            .checked_add(row.height)?
            .checked_add(reserve_at(row, page, tail)?)
    })
}

fn table_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::DataType, message)
}

fn reserve_at(row: &MeasuredRow, page: usize, tail: Option<&TailReservation>) -> Result<Unit> {
    if let Some(tail) = tail {
        tail.control
            .checkpoint(crate::ProgressPhase::Layout, 0, None)?;
    }
    row.reserve_after.checked_add(if row.final_row {
        tail.map_or(Unit::ZERO, |t| t.at(page))
    } else {
        Unit::ZERO
    })
}

fn fit_current(
    spec: &TableSpec,
    state: &mut PaginationState,
    tail: Option<&TailReservation>,
    rows: Vec<MeasuredRow>,
    capacity: Unit,
) -> Result<bool> {
    if !state.can_fit(rows_height(&rows, state.page_index(), tail)?, capacity)? {
        return Ok(false);
    }
    record(tail, state, &rows, capacity, crate::PaginationRule::Fit)?;
    for row in rows {
        let reserve = reserve_at(&row, state.page_index(), tail)?;
        state.push(spec, &row.row, row.height, reserve)?;
    }
    Ok(true)
}
