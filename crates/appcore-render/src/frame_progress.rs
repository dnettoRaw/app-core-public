// =============================================================================
//        #######
//     ###       ###     F: frame_progress.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use crate::RenderError;
use std::sync::atomic::{AtomicU64, Ordering};
/// Progress local to one renderer/device generation. Zero means no submitted frame.
/// Queue completion is not compositor presentation or physical display scanout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeFrameProgress {
    /// Last successfully submitted presentation frame ID.
    pub submitted: u64,
    /// Latest submitted frame whose queue completion callback was observed.
    pub completed: u64,
}
#[derive(Default)]
pub(crate) struct FrameCompletion {
    submitted: AtomicU64,
    completed: AtomicU64,
}
impl FrameCompletion {
    /// Renderer is main-thread owned; only its completion callbacks may race.
    pub(crate) fn next(&self) -> Result<u64, RenderError> {
        self.submitted
            .load(Ordering::Acquire)
            .checked_add(1)
            .ok_or(RenderError::IdentityExhausted)
    }
    pub(crate) fn submitted(&self, id: u64) {
        self.submitted.store(id, Ordering::Release);
    }
    pub(crate) fn completed(&self, id: u64) {
        self.completed.fetch_max(id, Ordering::AcqRel);
    }
    pub(crate) fn snapshot(&self) -> NativeFrameProgress {
        let completed = self.completed.load(Ordering::Acquire);
        NativeFrameProgress {
            submitted: self.submitted.load(Ordering::Acquire),
            completed,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn completion_is_monotonic_and_generation_local() -> Result<(), RenderError> {
        let tracker = FrameCompletion::default();
        assert_eq!(tracker.next()?, 1);
        tracker.submitted(1);
        tracker.submitted(2);
        tracker.completed(2);
        tracker.completed(1);
        assert_eq!(
            tracker.snapshot(),
            NativeFrameProgress {
                submitted: 2,
                completed: 2
            }
        );
        assert_eq!(
            FrameCompletion::default().snapshot(),
            NativeFrameProgress {
                submitted: 0,
                completed: 0
            }
        );
        tracker.submitted(u64::MAX);
        assert_eq!(tracker.next(), Err(RenderError::IdentityExhausted));
        Ok(())
    }
}
