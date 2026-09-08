// =============================================================================
//        #######
//     ###       ###     F: candle_memory.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/03 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/03 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.3
// =============================================================================

//! Owns Candle model-load admission and lifetime-bound memory accounting.

use crate::{AiError, AiResult};
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
struct State {
    current_models: usize,
    current_bytes: u64,
    peak_models: usize,
    peak_bytes: u64,
    rejected_loads: u64,
}

/// Payload-free pressure for loaded and currently loading Candle models.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CandleMemoryPressure {
    /// Models loading or retained by the backend and active inference leases.
    pub current_models: usize,
    /// Declared artifact bytes for current models.
    pub current_bytes: u64,
    /// Highest simultaneous accounted model count.
    pub peak_models: usize,
    /// Highest simultaneous declared artifact bytes.
    pub peak_bytes: u64,
    /// Maximum simultaneously accounted models.
    pub max_models: usize,
    /// Maximum simultaneous declared artifact bytes.
    pub max_bytes: u64,
    /// Loads rejected before reading or decoding an artifact.
    pub rejected_loads: u64,
}

#[derive(Debug)]
pub(super) struct CandleLoadBudget {
    max_models: usize,
    max_bytes: u64,
    state: Mutex<State>,
}

impl CandleLoadBudget {
    pub(super) fn new(max_models: usize, max_bytes: u64) -> Self {
        Self {
            max_models,
            max_bytes,
            state: Mutex::new(State::default()),
        }
    }

    pub(super) fn reserve(self: &Arc<Self>, bytes: u64) -> AiResult<CandleLoadReservation> {
        let mut state = self.state.lock().map_err(|_| AiError::InternalState)?;
        let next_models = state.current_models.saturating_add(1);
        let Some(next_bytes) = state.current_bytes.checked_add(bytes) else {
            return reject(&mut state);
        };
        if next_models > self.max_models || next_bytes > self.max_bytes {
            return reject(&mut state);
        }
        state.current_models = next_models;
        state.current_bytes = next_bytes;
        state.peak_models = state.peak_models.max(next_models);
        state.peak_bytes = state.peak_bytes.max(next_bytes);
        Ok(CandleLoadReservation {
            budget: Arc::clone(self),
            bytes,
        })
    }

    pub(super) fn pressure(&self) -> AiResult<CandleMemoryPressure> {
        let state = self.state.lock().map_err(|_| AiError::InternalState)?;
        Ok(CandleMemoryPressure {
            current_models: state.current_models,
            current_bytes: state.current_bytes,
            peak_models: state.peak_models,
            peak_bytes: state.peak_bytes,
            max_models: self.max_models,
            max_bytes: self.max_bytes,
            rejected_loads: state.rejected_loads,
        })
    }

    fn release(&self, bytes: u64) {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        state.current_models = state.current_models.saturating_sub(1);
        state.current_bytes = state.current_bytes.saturating_sub(bytes);
    }
}

pub(super) struct CandleLoadReservation {
    budget: Arc<CandleLoadBudget>,
    bytes: u64,
}

impl Drop for CandleLoadReservation {
    fn drop(&mut self) {
        self.budget.release(self.bytes);
    }
}

fn reject<T>(state: &mut State) -> AiResult<T> {
    state.rejected_loads = state.rejected_loads.saturating_add(1);
    Err(AiError::Capacity("Candle loaded model memory"))
}
