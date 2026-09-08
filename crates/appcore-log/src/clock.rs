// =============================================================================
//        #######
//     ###       ###     F: clock.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: working-tree by dnettoRaw
//    ##   ## ##   ##    U: working-tree by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Explicit clocks keep event timestamps deterministic at test boundaries.

/// Supplies Unix timestamps in milliseconds at an application boundary.
pub trait LogClock: Send + Sync {
    /// Returns the current Unix timestamp in milliseconds.
    fn now_ms(&self) -> u64;
}

/// Production wall-clock implementation with a controlled pre-epoch fallback.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemLogClock;

impl LogClock for SystemLogClock {
    fn now_ms(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis().try_into().unwrap_or(u64::MAX))
            .unwrap_or_default()
    }
}

/// Deterministic clock for tests and reproducible examples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedLogClock(u64);

impl FixedLogClock {
    /// Creates a clock returning exactly `timestamp_ms`.
    pub const fn new(timestamp_ms: u64) -> Self {
        Self(timestamp_ms)
    }
}

impl LogClock for FixedLogClock {
    fn now_ms(&self) -> u64 {
        self.0
    }
}
