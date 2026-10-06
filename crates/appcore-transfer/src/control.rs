// =============================================================================
//        #######
//     ###       ###     F: control.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/06 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/06 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Cooperative cancellation with a checked monotonic deadline.
use crate::{TransferError as E, TransferResult};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
/// Shared cancellation/deadline for all bounded workers; never starts background tasks.
#[derive(Debug, Clone)]
pub struct TransferControl {
    cancelled: Arc<AtomicBool>,
    deadline: Instant,
}
impl TransferControl {
    /// Creates a deadline between 1 ms and 24 hours.
    pub fn new(timeout: Duration) -> TransferResult<Self> {
        if timeout < Duration::from_millis(1) || timeout > Duration::from_secs(86400) {
            return Err(E::InvalidContract);
        }
        Ok(Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            deadline: Instant::now()
                .checked_add(timeout)
                .ok_or(E::InvalidContract)?,
        })
    }
    /// Requests cooperative cancellation; providers must return within their I/O bounds.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
    /// Returns a typed cancellation/deadline failure.
    pub fn checkpoint(&self) -> TransferResult<()> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err(E::Cancelled);
        }
        if Instant::now() >= self.deadline {
            return Err(E::Deadline);
        }
        Ok(())
    }
    /// Remaining monotonic deadline, for deployment-owned transport timeouts.
    pub fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }
    pub(crate) fn wait(&self, delay: Duration) -> TransferResult<()> {
        let until = Instant::now()
            .checked_add(delay)
            .ok_or(E::InvalidContract)?;
        while Instant::now() < until {
            self.checkpoint()?;
            std::thread::sleep(
                until
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(10)),
            );
        }
        self.checkpoint()
    }
}
