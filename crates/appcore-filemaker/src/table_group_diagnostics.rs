// =============================================================================
//        #######
//     ###       ###     F: table_group_diagnostics.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/17 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/17 12:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

//! Numeric decision recording without retaining row values.
use super::{rows_height, MeasuredRow};
use crate::table::PaginationState;
use crate::table_tail::TailReservation;
use crate::{Result, Unit};

pub(super) fn record(
    tail: Option<&TailReservation>,
    state: &PaginationState,
    rows: &[MeasuredRow],
    capacity: Unit,
    rule: crate::PaginationRule,
) -> Result<()> {
    record_at(
        tail,
        state.page_index(),
        state.used_height(),
        rows,
        capacity,
        rule,
    )
}

pub(super) fn record_at(
    tail: Option<&TailReservation>,
    page: usize,
    used_height: Unit,
    rows: &[MeasuredRow],
    capacity: Unit,
    rule: crate::PaginationRule,
) -> Result<()> {
    let Some(tail) = tail else { return Ok(()) };
    let Some(recorder) = &tail.diagnostics else {
        return Ok(());
    };
    let Some(first) = rows.first() else {
        return Ok(());
    };
    let Some(last) = rows.last() else {
        return Ok(());
    };
    let group_height = rows
        .iter()
        .try_fold(Unit::ZERO, |sum, row| sum.checked_add(row.height))?;
    let required_height = rows_height(rows, page, Some(tail))?;
    let header_height = if page == 0 || tail.repeat_header {
        tail.header_height
    } else {
        Unit::ZERO
    };
    recorder.push(crate::PaginationDecision {
        table: tail.table.clone(),
        page,
        first_row: first.source_index,
        last_row: last.source_index,
        rule,
        capacity,
        header_height,
        body_height: capacity.checked_add(header_height)?,
        used_height,
        available_height: capacity.checked_sub(used_height)?,
        group_height,
        reserved_height: required_height.checked_sub(group_height)?,
        final_reservation: if last.final_row {
            tail.at(page)
        } else {
            Unit::ZERO
        },
        required_height,
    })
}

pub(super) fn oversize(
    tail: Option<&TailReservation>,
    page: usize,
    rows: &[MeasuredRow],
    capacity: Unit,
) -> Result<bool> {
    let reject = tail.is_some_and(|t| t.oversized == crate::adaptive_layout::OversizedGroup::Error);
    let rule = if reject {
        crate::PaginationRule::Reject
    } else {
        crate::PaginationRule::SplitRows
    };
    record_at(tail, page, Unit::ZERO, rows, capacity, rule)?;
    Ok(reject)
}
