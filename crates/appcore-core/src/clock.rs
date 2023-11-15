// =============================================================================
//        #######
//     ###       ###     F: clock.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/09 08:35:21 by dnettoRaw
//    ##   ## ##   ##    U: 2026/06/09 08:35:21 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Mockable clock utilities for retrieving standard timestamps.

use std::time::{SystemTime, UNIX_EPOCH};

/// Mockable clock trait for fetching millisecond timestamps.
pub trait Clock: Send + Sync + std::fmt::Debug {
    /// Returns the current epoch time in milliseconds.
    fn now_ms(&self) -> u64;
}

/// Standard system clock implementation using `std::time::SystemTime`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl SystemClock {
    /// Create a new instance of the system clock.
    pub fn new() -> Self {
        Self
    }
}

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}
