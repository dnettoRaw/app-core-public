// =============================================================================
//        #######
//     ###       ###     F: frame_admission.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/14 16:20:57 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-alpha.1
// =============================================================================

use crate::RenderError;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

/// Owns one of two GPU submission slots. A pending encode releases on failure/drop;
/// a submitted frame transfers ownership to the queue completion callback.
pub(crate) struct FramePermit(Arc<AtomicU32>);
impl FramePermit {
    pub(crate) fn acquire(count: &Arc<AtomicU32>) -> Result<Self, RenderError> {
        count
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < 2).then_some(n + 1)
            })
            .map_err(|_| RenderError::FramesPending)?;
        Ok(Self(count.clone()))
    }
}
impl Drop for FramePermit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn completion_and_failed_encoding_release_exactly_one_slot() -> Result<(), RenderError> {
        let count = Arc::new(AtomicU32::new(0));
        let a = FramePermit::acquire(&count)?;
        let b = FramePermit::acquire(&count)?;
        assert!(matches!(
            FramePermit::acquire(&count),
            Err(RenderError::FramesPending)
        ));
        drop(a);
        assert_eq!(count.load(Ordering::Acquire), 1);
        let c = FramePermit::acquire(&count)?;
        drop(b);
        drop(c);
        assert_eq!(count.load(Ordering::Acquire), 0);
        Ok(())
    }
    #[test]
    fn racing_admission_never_exceeds_two_pending_slots() {
        let count = Arc::new(AtomicU32::new(0));
        let barrier = Arc::new(std::sync::Barrier::new(17));
        let accepted = Arc::new(AtomicU32::new(0));
        std::thread::scope(|scope| {
            for _ in 0..16 {
                let (count, barrier, accepted) = (count.clone(), barrier.clone(), accepted.clone());
                scope.spawn(move || {
                    let permit = FramePermit::acquire(&count);
                    if permit.is_ok() {
                        accepted.fetch_add(1, Ordering::AcqRel);
                    }
                    barrier.wait();
                    barrier.wait();
                    drop(permit);
                });
            }
            barrier.wait();
            assert_eq!(accepted.load(Ordering::Acquire), 2);
            assert_eq!(count.load(Ordering::Acquire), 2);
            barrier.wait();
        });
        assert_eq!(count.load(Ordering::Acquire), 0);
    }
}
