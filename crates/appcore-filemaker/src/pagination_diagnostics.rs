// =============================================================================
//        #######
//     ###       ###     F: pagination_diagnostics.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/17 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/17 12:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

//! Opt-in bounded pagination evidence, retained even when layout fails.
use crate::{ElementId, ErrorCode, FileMakerError, ResolvedScene, Result, Unit};
use serde::Serialize;
use std::sync::{Arc, Mutex};

/// Actual action taken by the adaptive table paginator.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PaginationRule {
    /// The group or row fits the current page.
    Fit,
    /// Keep a group intact by advancing to the next page.
    MoveGroup,
    /// Apply the explicitly selected row splitting fallback.
    SplitRows,
    /// Advance before placing an individual row.
    MoveRow,
    /// Defer an empty first table page; adaptive validation may reject it.
    SkipFirstPage,
    /// The group or row cannot fit under the selected policy.
    Reject,
}

/// Numeric decision evidence. Indexes are zero-based; no data values are retained.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[non_exhaustive]
pub struct PaginationDecision {
    /// Table source ID.
    pub table: ElementId,
    /// Physical page being evaluated (the adaptive table starts on page zero).
    pub page: usize,
    /// Inclusive first source row ordinal.
    pub first_row: u64,
    /// Inclusive last source row ordinal.
    pub last_row: u64,
    /// Action selected for this candidate.
    pub rule: PaginationRule,
    /// Body height after subtracting the repeated header exactly once.
    pub capacity: Unit,
    /// Header reservation applied to this candidate page.
    pub header_height: Unit,
    /// Normal body height before subtracting the header or final reservation.
    pub body_height: Unit,
    /// Space consumed by earlier rows and their reservations.
    pub used_height: Unit,
    /// Capacity minus used height before this decision.
    pub available_height: Unit,
    /// Sum of measured row heights, excluding reservations.
    pub group_height: Unit,
    /// Total candidate reservation, including final-region reduction if applicable.
    pub reserved_height: Unit,
    /// Final-region reduction (closing height/gap plus body-height difference).
    pub final_reservation: Unit,
    /// Measured group height plus all candidate reservations.
    pub required_height: Unit,
}

/// A layout result and its bounded evidence, including evidence preceding failure.
/// A diagnostic budget exhaustion fails layout explicitly; records are never silently dropped.
#[must_use = "inspect the layout outcome and diagnostic evidence"]
pub struct AdaptiveLayoutReport {
    pub(crate) outcome: Result<ResolvedScene>,
    pub(crate) decisions: Vec<PaginationDecision>,
}
impl AdaptiveLayoutReport {
    /// Returns the scene or the original layout/diagnostic error.
    pub fn outcome(&self) -> &Result<ResolvedScene> {
        &self.outcome
    }
    /// Returns decisions in execution order. Records contain no document text or group keys.
    pub fn decisions(&self) -> &[PaginationDecision] {
        &self.decisions
    }
    /// Consumes the report, returning the layout result.
    pub fn into_outcome(self) -> Result<ResolvedScene> {
        self.outcome
    }
}

#[derive(Clone)]
pub(crate) struct DecisionRecorder {
    entries: Arc<Mutex<Vec<PaginationDecision>>>,
    limit: usize,
}
impl DecisionRecorder {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            entries: Arc::new(Mutex::new(Vec::new())),
            limit,
        }
    }
    pub(crate) fn push(&self, decision: PaginationDecision) -> Result<()> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| diagnostic_error("pagination diagnostic lock failed"))?;
        if entries.len() >= self.limit {
            return Err(diagnostic_error("pagination decision limit exceeded"));
        }
        entries.push(decision);
        Ok(())
    }
    pub(crate) fn snapshot(&self) -> Result<Vec<PaginationDecision>> {
        self.entries
            .lock()
            .map(|entries| entries.clone())
            .map_err(|_| diagnostic_error("pagination diagnostic lock failed"))
    }
}
pub(crate) fn diagnostic_error(message: &str) -> FileMakerError {
    FileMakerError::new(ErrorCode::LimitExceeded, message)
}
