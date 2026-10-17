// =============================================================================
//        #######
//     ###       ###     F: input_metrics.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:11:39 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 0.1.0-beta.2
// =============================================================================

//! Bounded local latency samples; observed GPU completion is not display scanout.
use appcore_render::NativeFrameProgress;
use appcore_ui::UiNativeInputTiming;
use std::time::Duration;
const CAPACITY: usize = 256;
struct Samples {
    values: [u64; CAPACITY],
    count: usize,
    overflow: u64,
}
impl Default for Samples {
    fn default() -> Self {
        Self {
            values: [0; CAPACITY],
            count: 0,
            overflow: 0,
        }
    }
}
impl Samples {
    fn add(&mut self, duration: Duration) {
        if self.count == CAPACITY || duration.as_micros() > u128::from(u64::MAX) {
            self.overflow = self.overflow.saturating_add(1);
            return;
        }
        self.values[self.count] = duration.as_micros() as u64;
        self.count += 1;
    }
    fn percentiles(&self) -> Option<[u64; 3]> {
        if self.count == 0 {
            return None;
        }
        let mut sorted = self.values;
        sorted[..self.count].sort_unstable();
        Some([50, 95, 99].map(|p| sorted[(self.count * p).div_ceil(100) - 1]))
    }
    fn report(&self, label: &str) {
        println!(
            "latency {label} n={} p50_p95_p99_us={:?} overflow={}",
            self.count,
            self.percentiles(),
            self.overflow
        );
    }
}
#[derive(Clone, Copy)]
struct Pending {
    timing: UiNativeInputTiming,
    frame: Option<u64>,
}
pub struct InputMetrics {
    callback_action: Samples,
    input_action: Samples,
    input_present: Samples,
    input_completed: Samples,
    pending: [Option<Pending>; CAPACITY],
    uncorrelated: u64,
    pending_overflow: u64,
}
impl Default for InputMetrics {
    fn default() -> Self {
        Self {
            callback_action: Default::default(),
            input_action: Default::default(),
            input_present: Default::default(),
            input_completed: Default::default(),
            pending: [None; CAPACITY],
            uncorrelated: 0,
            pending_overflow: 0,
        }
    }
}
impl InputMetrics {
    /// Record after the reducer applies the action; retain only local timing metadata.
    pub fn applied(&mut self, timing: UiNativeInputTiming) {
        self.callback_action.add(timing.callback_elapsed());
        if let Some(elapsed) = timing.input_elapsed() {
            self.input_action.add(elapsed);
        } else {
            self.uncorrelated = self.uncorrelated.saturating_add(1);
        }
        if let Some(slot) = self.pending.iter_mut().find(|s| s.is_none()) {
            *slot = Some(Pending {
                timing,
                frame: None,
            });
        } else {
            self.pending_overflow = self.pending_overflow.saturating_add(1);
        }
    }
    /// First submitted frame after action consumption; intermediate states may be coalesced.
    pub fn presented(&mut self, frame: u64) {
        for pending in self
            .pending
            .iter_mut()
            .flatten()
            .filter(|p| p.frame.is_none())
        {
            pending.frame = Some(frame);
            if let Some(elapsed) = pending.timing.input_elapsed() {
                self.input_present.add(elapsed);
            }
        }
    }
    /// Observation includes device-poll/loop delay, not GPU-only execution time.
    pub fn observe(&mut self, progress: NativeFrameProgress) {
        for slot in &mut self.pending {
            if let Some(pending) = slot.filter(|p| p.frame.is_some_and(|f| f <= progress.completed))
            {
                if let Some(elapsed) = pending.timing.input_elapsed() {
                    self.input_completed.add(elapsed);
                }
                *slot = None;
            }
        }
    }
    pub fn report(&self) {
        self.callback_action.report("callback_to_action");
        self.input_action.report("event_to_action");
        self.input_present.report("event_to_present_call");
        self.input_completed
            .report("event_to_gpu_completion_observed");
        println!(
            "latency uncorrelated={} pending={} pending_overflow={}",
            self.uncorrelated,
            self.pending.iter().filter(|p| p.is_some()).count(),
            self.pending_overflow
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nearest_rank_percentiles_and_overflow_are_explicit() {
        let mut samples = Samples::default();
        assert_eq!(samples.percentiles(), None);
        for us in [9, 1, 8, 2, 7, 3, 6, 4, 5, 0] {
            samples.add(Duration::from_micros(us));
        }
        assert_eq!(samples.percentiles(), Some([4, 9, 9]));
        for _ in 10..CAPACITY + 1 {
            samples.add(Duration::ZERO);
        }
        assert_eq!(samples.count, CAPACITY);
        assert_eq!(samples.overflow, 1);
    }
}
