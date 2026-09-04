// =============================================================================
//        #######
//     ###       ###     F: progress.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use super::core::{selected_pages, ExportRequest};
use crate::{ErrorCode, FileMakerError, OperationControl, ProgressPhase, ResolvedScene, Result};

pub(super) struct ExportProgress<'a> {
    control: Option<&'a OperationControl>,
    completed: u64,
    total: u64,
}

impl<'a> ExportProgress<'a> {
    pub(super) fn new(
        scene: &ResolvedScene,
        request: &ExportRequest,
        control: Option<&'a OperationControl>,
    ) -> Result<Self> {
        let pages = selected_pages(scene, request)?;
        let total = pages
            .iter()
            .try_fold(0_u64, |count, page| {
                count.checked_add(u64::try_from(page.elements.len()).ok()?)
            })
            .ok_or_else(|| {
                FileMakerError::new(ErrorCode::LimitExceeded, "export element count overflow")
            })?;
        let progress = Self {
            control,
            completed: 0,
            total,
        };
        progress.checkpoint()?;
        Ok(progress)
    }

    pub(super) fn checkpoint(&self) -> Result<()> {
        if let Some(control) = self.control {
            control.checkpoint(ProgressPhase::Export, self.completed, Some(self.total))?;
        }
        Ok(())
    }

    pub(super) fn element(&mut self) -> Result<()> {
        self.completed = self.completed.checked_add(1).ok_or_else(|| {
            FileMakerError::new(ErrorCode::LimitExceeded, "export progress count overflow")
        })?;
        self.checkpoint()
    }

    pub(super) fn finish(&self) -> Result<()> {
        if self.completed != self.total {
            return Err(FileMakerError::new(
                ErrorCode::Validation,
                "export progress did not visit every resolved element",
            ));
        }
        Ok(())
    }
}
