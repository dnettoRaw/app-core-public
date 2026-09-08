// =============================================================================
//        #######
//     ###       ###     F: candle_execution.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/03 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/03 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.3
// =============================================================================

//! Keeps Candle execution helpers separate from model ownership and admission.

use crate::{AiError, AiResult, CancellationToken};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

pub(super) struct ActiveInference<'a> {
    active: &'a AtomicUsize,
}

impl<'a> ActiveInference<'a> {
    pub(super) fn new(active: &'a AtomicUsize) -> Self {
        active.fetch_add(1, Ordering::Relaxed);
        Self { active }
    }
}

impl Drop for ActiveInference<'_> {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::Relaxed);
    }
}

pub(super) fn text_features(text: &str, dimensions: usize) -> Vec<f32> {
    let mut features = vec![0.0; dimensions];
    for (index, byte) in text.bytes().enumerate() {
        let slot = (index.saturating_mul(257) ^ usize::from(byte)) % dimensions;
        features[slot] += 1.0;
    }
    let divisor = text.len().max(1) as f32;
    for value in &mut features {
        *value /= divisor;
    }
    features
}

pub(super) fn check_cancellation(cancellation: &CancellationToken) -> AiResult<()> {
    if cancellation.is_cancelled() {
        Err(AiError::Cancelled)
    } else {
        Ok(())
    }
}

pub(super) fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis())
        .unwrap_or(u64::MAX)
        .max(1)
}

pub(super) fn update_ema(target: &AtomicU64, sample: u64) {
    let mut previous = target.load(Ordering::Relaxed);
    loop {
        let next = if previous == 0 {
            sample
        } else {
            previous.saturating_mul(4).saturating_add(sample) / 5
        };
        match target.compare_exchange_weak(previous, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(observed) => previous = observed,
        }
    }
}
