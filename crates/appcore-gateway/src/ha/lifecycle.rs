// =============================================================================
//        #######
//     ###       ###     F: lifecycle.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================

//! Lock-free explicit admission lifecycle for opt-in Gateway HA.

use super::{GatewayRegistryError, GatewayRegistryResult};
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};

const MODE_STOPPED: u8 = 0;
const MODE_RECOVERING: u8 = 1;
const MODE_HEALTHY: u8 = 2;
const MODE_ISOLATED: u8 = 3;

/// Explicit operational mode of an opt-in HA Gateway instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayHaMode {
    /// No HA work is admitted and no recovery is active.
    Stopped,
    /// A fresh epoch and the complete local ownership snapshot are being acquired.
    Recovering,
    /// Shared ownership is current and HA-dependent work may be admitted.
    Healthy,
    /// Registry reachability or ownership is uncertain; all HA work fails closed.
    Isolated,
}

impl GatewayHaMode {
    const fn from_raw(value: u8) -> Self {
        match value {
            MODE_RECOVERING => Self::Recovering,
            MODE_HEALTHY => Self::Healthy,
            MODE_ISOLATED => Self::Isolated,
            _ => Self::Stopped,
        }
    }

    const fn raw(self) -> u8 {
        match self {
            Self::Stopped => MODE_STOPPED,
            Self::Recovering => MODE_RECOVERING,
            Self::Healthy => MODE_HEALTHY,
            Self::Isolated => MODE_ISOLATED,
        }
    }
}

/// Safe bounded HA lifecycle telemetry snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GatewayHaLifecycleSnapshot {
    /// Current explicit mode.
    pub mode: GatewayHaMode,
    /// Successful mode changes.
    pub transitions: u64,
    /// Recovery attempts started.
    pub recoveries_started: u64,
    /// Transitions caused by uncertain registry state.
    pub isolations: u64,
    /// Fencing rejections observed by the owner.
    pub fencing_rejections: u64,
    /// Duration of the last completed recovery.
    pub last_recovery_duration_ms: u64,
}

/// Lock-free mode and counters shared by HTTP, socket and provider work.
pub struct GatewayHaLifecycle {
    mode: AtomicU8,
    transitions: AtomicU64,
    recoveries_started: AtomicU64,
    isolations: AtomicU64,
    fencing_rejections: AtomicU64,
    recovery_started_ms: AtomicU64,
    last_recovery_duration_ms: AtomicU64,
}

impl Default for GatewayHaLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

impl GatewayHaLifecycle {
    /// Creates a stopped lifecycle that rejects work until recovery completes.
    pub const fn new() -> Self {
        Self {
            mode: AtomicU8::new(MODE_STOPPED),
            transitions: AtomicU64::new(0),
            recoveries_started: AtomicU64::new(0),
            isolations: AtomicU64::new(0),
            fencing_rejections: AtomicU64::new(0),
            recovery_started_ms: AtomicU64::new(0),
            last_recovery_duration_ms: AtomicU64::new(0),
        }
    }

    /// Starts bounded recovery from stopped or isolated state.
    pub fn begin_recovery(&self, now_ms: u64) -> GatewayRegistryResult<()> {
        loop {
            let current = self.mode.load(Ordering::Acquire);
            match GatewayHaMode::from_raw(current) {
                GatewayHaMode::Recovering => return Ok(()),
                GatewayHaMode::Healthy => return Err(GatewayRegistryError::InvalidContract),
                GatewayHaMode::Stopped | GatewayHaMode::Isolated => {}
            }
            if self
                .mode
                .compare_exchange(
                    current,
                    MODE_RECOVERING,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                self.recovery_started_ms.store(now_ms, Ordering::Release);
                increment(&self.recoveries_started);
                increment(&self.transitions);
                return Ok(());
            }
        }
    }

    /// Completes recovery only from the recovering state.
    pub fn mark_healthy(&self, now_ms: u64) -> GatewayRegistryResult<()> {
        if self
            .mode
            .compare_exchange(
                MODE_RECOVERING,
                MODE_HEALTHY,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
        {
            return Err(GatewayRegistryError::InvalidContract);
        }
        let started_ms = self.recovery_started_ms.swap(0, Ordering::AcqRel);
        self.last_recovery_duration_ms
            .store(now_ms.saturating_sub(started_ms), Ordering::Release);
        increment(&self.transitions);
        Ok(())
    }

    /// Enters isolated mode after registry or ownership uncertainty.
    pub fn isolate(&self) -> GatewayRegistryResult<()> {
        loop {
            let current = self.mode.load(Ordering::Acquire);
            match GatewayHaMode::from_raw(current) {
                GatewayHaMode::Isolated => return Ok(()),
                GatewayHaMode::Stopped => return Err(GatewayRegistryError::InvalidContract),
                GatewayHaMode::Recovering | GatewayHaMode::Healthy => {}
            }
            if self
                .mode
                .compare_exchange(current, MODE_ISOLATED, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                self.recovery_started_ms.store(0, Ordering::Release);
                increment(&self.isolations);
                increment(&self.transitions);
                return Ok(());
            }
        }
    }

    /// Stops admission from any current mode.
    pub fn stop(&self) {
        let previous = self.mode.swap(MODE_STOPPED, Ordering::AcqRel);
        self.recovery_started_ms.store(0, Ordering::Release);
        if previous != MODE_STOPPED {
            increment(&self.transitions);
        }
    }

    /// Allows HA-dependent work only while the shared owner is healthy.
    pub fn admit(&self) -> GatewayRegistryResult<()> {
        if self.mode.load(Ordering::Acquire) == GatewayHaMode::Healthy.raw() {
            return Ok(());
        }
        Err(GatewayRegistryError::Unavailable)
    }

    /// Records one exact stale owner or generation rejection.
    pub fn record_fencing_rejection(&self) {
        increment(&self.fencing_rejections);
    }

    /// Returns a safe point-in-time lifecycle snapshot.
    pub fn snapshot(&self) -> GatewayHaLifecycleSnapshot {
        GatewayHaLifecycleSnapshot {
            mode: GatewayHaMode::from_raw(self.mode.load(Ordering::Acquire)),
            transitions: self.transitions.load(Ordering::Relaxed),
            recoveries_started: self.recoveries_started.load(Ordering::Relaxed),
            isolations: self.isolations.load(Ordering::Relaxed),
            fencing_rejections: self.fencing_rejections.load(Ordering::Relaxed),
            last_recovery_duration_ms: self.last_recovery_duration_ms.load(Ordering::Relaxed),
        }
    }
}

fn increment(counter: &AtomicU64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        Some(value.saturating_add(1))
    });
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
