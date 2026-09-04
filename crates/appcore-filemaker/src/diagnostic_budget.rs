// =============================================================================
//        #######
//     ###       ###     F: diagnostic_budget.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use crate::{ErrorCode, FileMakerError, ResourceLimits, Result};

pub(crate) struct DiagnosticBudget {
    operations: usize,
    limit: usize,
}

impl DiagnosticBudget {
    pub(crate) fn new(limits: &ResourceLimits) -> Result<Self> {
        limits.validate()?;
        Ok(Self {
            operations: 0,
            limit: limits.max_preflight_comparisons,
        })
    }

    pub(crate) fn operation(&mut self) -> Result<()> {
        self.operations = self
            .operations
            .checked_add(1)
            .ok_or_else(|| budget_error("diagnostic geometry operation count overflow"))?;
        if self.operations > self.limit {
            return Err(budget_error(
                "diagnostic geometry operation budget exhausted",
            ));
        }
        Ok(())
    }

    pub(crate) fn retained(&self, count: usize) -> Result<()> {
        if count > self.limit {
            return Err(budget_error(
                "diagnostic retained geometry budget exhausted",
            ));
        }
        Ok(())
    }
}

fn budget_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::LimitExceeded, message)
}
