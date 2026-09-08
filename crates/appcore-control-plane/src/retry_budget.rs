// =============================================================================
//        #######
//     ###       ###     F: retry_budget.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/07 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/07 00:00:00 by dnettoRaw
//      ###########      S: 1.0.3-rc
// =============================================================================

//! Monotonic, bounded retry-cycle accounting independent of transport cooperation.

use crate::{ControlPlaneError, ControlPlaneHttpConfig, ControlPlaneResult};
use std::time::{Duration, Instant};

pub(crate) struct RetryBudget {
    started: Instant,
    duration: Duration,
    jitter_state: u64,
}

impl RetryBudget {
    pub(crate) fn new_at(
        config: &ControlPlaneHttpConfig,
        started: Instant,
    ) -> ControlPlaneResult<Self> {
        let attempts = config.retry_policy.max_attempts.max(1);
        if attempts > 16
            || config.timeout_ms == 0
            || config.timeout_ms > 30_000
            || config.retry_policy.initial_backoff_ms > 30_000
            || config.retry_policy.max_backoff_ms > 30_000
        {
            return Err(ControlPlaneError::Rejected(
                "retry configuration exceeds limits".to_string(),
            ));
        }
        let total_ms = config.timeout_ms * attempts as u64
            + config.retry_policy.max_backoff_ms * (attempts - 1) as u64;
        if total_ms > 120_000 {
            return Err(ControlPlaneError::Rejected(
                "retry cycle exceeds 120 seconds".to_string(),
            ));
        }
        Ok(Self {
            started,
            duration: Duration::from_millis(total_ms),
            jitter_state: std::hash::BuildHasher::hash_one(
                &std::collections::hash_map::RandomState::new(),
                0_u8,
            ),
        })
    }

    pub(crate) fn remaining(&self) -> ControlPlaneResult<Duration> {
        self.duration
            .checked_sub(self.started.elapsed())
            .filter(|remaining| !remaining.is_zero())
            .ok_or(ControlPlaneError::Timeout)
    }

    pub(crate) fn attempt_timeout_ms(&self, configured: u64) -> ControlPlaneResult<u64> {
        let millis = self.remaining()?.as_millis() as u64;
        if millis == 0 {
            return Err(ControlPlaneError::Timeout);
        }
        Ok(configured.min(millis))
    }

    pub(crate) fn retry_delay(&mut self, maximum_ms: u64) -> ControlPlaneResult<Duration> {
        let millis = jitter_delay(&mut self.jitter_state, maximum_ms);
        Ok(Duration::from_millis(millis).min(self.remaining()?))
    }
}

fn jitter_delay(state: &mut u64, maximum_ms: u64) -> u64 {
    // Non-cryptographic scheduling jitter: a fixed seed makes tests reproducible.
    // Equal jitter retains half the exponential delay to avoid retry hot loops.
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut mixed = (*state ^ (*state >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    mixed ^= mixed >> 31;
    let minimum = maximum_ms / 2 + maximum_ms % 2;
    minimum + mixed % (maximum_ms - minimum + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jitter_is_reproducible_varied_and_never_exceeds_backoff() {
        let (mut first, mut second) = (42, 42);
        let mut distinct = std::collections::HashSet::new();
        for maximum in [0, 1, 2, 50, 500, 30_000] {
            for _ in 0..100 {
                let delay = jitter_delay(&mut first, maximum);
                assert_eq!(delay, jitter_delay(&mut second, maximum));
                assert!((maximum / 2 + maximum % 2..=maximum).contains(&delay));
                if maximum == 500 {
                    distinct.insert(delay);
                }
            }
        }
        assert!(distinct.len() > 1);
    }

    #[test]
    fn retry_configuration_rejects_excess_and_accepts_exact_cycle_limit() {
        let mut config = ControlPlaneHttpConfig {
            base_url: "https://control.invalid".to_string(),
            timeout_ms: 30_000,
            retry_policy: crate::RetryPolicy {
                max_attempts: 3,
                initial_backoff_ms: 15_000,
                max_backoff_ms: 15_000,
            },
        };
        assert!(RetryBudget::new_at(&config, Instant::now()).is_ok());
        config.retry_policy.max_backoff_ms += 1;
        assert!(RetryBudget::new_at(&config, Instant::now()).is_err());
        config.retry_policy = crate::RetryPolicy::default();
        config.retry_policy.max_attempts = 17;
        assert!(RetryBudget::new_at(&config, Instant::now()).is_err());
        config.retry_policy.max_attempts = 1;
        config.timeout_ms = 0;
        assert!(RetryBudget::new_at(&config, Instant::now()).is_err());
        config.timeout_ms = 30_001;
        assert!(RetryBudget::new_at(&config, Instant::now()).is_err());
    }
}
