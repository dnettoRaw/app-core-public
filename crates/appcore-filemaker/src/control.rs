// =============================================================================
//        #######
//     ###       ###     F: control.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{ErrorCode, FileMakerError, Result};

/// Stable operation phase reported at cooperative boundaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressPhase {
    /// Parsing and expanding a template.
    Compile,
    /// Binding typed data and patches.
    Bind,
    /// Expanding bound and repeated element instances.
    BindElements,
    /// Measuring and resolving geometry.
    Layout,
    /// Collision/reflow iteration.
    Reflow,
    /// Preflight inspection.
    Preflight,
    /// Encoding output.
    Export,
}

/// Bounded progress notification.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProgressEvent {
    /// Current phase.
    pub phase: ProgressPhase,
    /// Completed deterministic work units.
    pub completed: u64,
    /// Known upper bound, when meaningful.
    pub total: Option<u64>,
}

/// Observer invoked synchronously; implementations must return promptly.
pub trait ProgressObserver: Send + Sync {
    /// Receives one progress event without influencing compiler behavior.
    fn report(&self, event: &ProgressEvent);
}

/// Cheap cloneable cooperative cancellation flag.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    /// Requests cancellation for every clone of this token.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Returns whether cancellation has been requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    /// Converts a cancellation request into the stable controlled error.
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            return Err(FileMakerError::new(
                ErrorCode::Cancelled,
                "operation cancelled at a cooperative boundary",
            ));
        }
        Ok(())
    }
}

/// Cancellation and progress controls shared by one operation pipeline.
#[derive(Clone, Default)]
pub struct OperationControl {
    cancellation: CancellationToken,
    observer: Option<Arc<dyn ProgressObserver>>,
}

impl OperationControl {
    /// Creates controls around the supplied cancellation token.
    #[must_use]
    pub fn new(cancellation: CancellationToken) -> Self {
        Self {
            cancellation,
            observer: None,
        }
    }

    /// Installs a synchronous progress observer.
    #[must_use]
    pub fn with_observer(mut self, observer: Arc<dyn ProgressObserver>) -> Self {
        self.observer = Some(observer);
        self
    }

    /// Returns the shared cancellation token.
    #[must_use]
    pub fn cancellation(&self) -> &CancellationToken {
        &self.cancellation
    }

    /// Checks cancellation, then emits a bounded progress event.
    pub fn checkpoint(
        &self,
        phase: ProgressPhase,
        completed: u64,
        total: Option<u64>,
    ) -> Result<()> {
        self.cancellation.check()?;
        if let Some(observer) = &self.observer {
            observer.report(&ProgressEvent {
                phase,
                completed,
                total,
            });
            self.cancellation.check()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_is_shared_and_controlled() {
        let token = CancellationToken::default();
        let clone = token.clone();
        token.cancel();
        assert_eq!(clone.check().unwrap_err().code(), ErrorCode::Cancelled);
    }
}
