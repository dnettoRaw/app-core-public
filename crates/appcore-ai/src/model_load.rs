// =============================================================================
//        #######
//     ###       ###     F: model_load.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{AiError, AiResult, BackendId, CancellationToken, ModelId};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

#[cfg(not(test))]
const MAX_TRACKED_ROUTES: usize = 4_096;
#[cfg(test)]
const MAX_TRACKED_ROUTES: usize = 8;
const CANCELLATION_POLL: Duration = Duration::from_millis(10);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LoadState {
    Loading,
    Ready { last_used: u64 },
}

#[derive(Debug, Default)]
struct State {
    routes: BTreeMap<(ModelId, BackendId), LoadState>,
    next_use: u64,
}

/// Bounded model-load single-flight counters and gauges.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ModelLoadSnapshot {
    /// Routes whose backend reports a completed load.
    pub ready: usize,
    /// Routes currently owned by one loader.
    pub loading: usize,
    /// Acquisitions satisfied without another load.
    pub ready_hits: u64,
    /// Acquisitions that waited behind a loader.
    pub waiters: u64,
    /// Callers elected as the single loader for a route.
    pub loaders: u64,
    /// Least-recently-used ready routes removed at the fixed bound.
    pub evictions: u64,
    /// Ready routes invalidated after backend unavailability.
    pub invalidations: u64,
}

#[derive(Debug, Default)]
pub(crate) struct ModelLoadCoordinator {
    state: Mutex<State>,
    changed: Condvar,
    ready_hits: AtomicU64,
    waiters: AtomicU64,
    loaders: AtomicU64,
    evictions: AtomicU64,
    invalidations: AtomicU64,
}

impl ModelLoadCoordinator {
    pub(crate) fn acquire(
        &self,
        model: &ModelId,
        backend: &BackendId,
        deadline: Option<Duration>,
        cancellation: &CancellationToken,
    ) -> AiResult<ModelLoadAdmission<'_>> {
        let started = Instant::now();
        let key = (model.clone(), backend.clone());
        let mut waited_for_loader = false;
        let mut state = self.state.lock().map_err(|_| AiError::InternalState)?;
        loop {
            if cancellation.is_cancelled() {
                return Err(AiError::Cancelled);
            }
            if deadline.is_some_and(|limit| started.elapsed() >= limit) {
                return Err(AiError::DeadlineExceeded);
            }
            let next_use = state.next_use.saturating_add(1);
            state.next_use = next_use;
            match state.routes.get_mut(&key) {
                Some(LoadState::Ready { last_used }) => {
                    *last_used = next_use;
                    self.ready_hits.fetch_add(1, Ordering::Relaxed);
                    return Ok(ModelLoadAdmission::Ready);
                }
                Some(LoadState::Loading) => {
                    if !waited_for_loader {
                        self.waiters.fetch_add(1, Ordering::Relaxed);
                        waited_for_loader = true;
                    }
                    let remaining = deadline
                        .map(|limit| limit.saturating_sub(started.elapsed()))
                        .unwrap_or(CANCELLATION_POLL)
                        .min(CANCELLATION_POLL);
                    let waited = self
                        .changed
                        .wait_timeout(state, remaining)
                        .map_err(|_| AiError::InternalState)?;
                    state = waited.0;
                }
                None => {
                    if state.routes.len() >= MAX_TRACKED_ROUTES {
                        let lru = state
                            .routes
                            .iter()
                            .filter_map(|(key, load)| match load {
                                LoadState::Ready { last_used } => Some((key.clone(), *last_used)),
                                LoadState::Loading => None,
                            })
                            .min_by_key(|(_, last_used)| *last_used)
                            .map(|(key, _)| key)
                            .ok_or(AiError::Capacity("tracked model routes are full"))?;
                        state.routes.remove(&lru);
                        self.evictions.fetch_add(1, Ordering::Relaxed);
                    }
                    state.routes.insert(key.clone(), LoadState::Loading);
                    self.loaders.fetch_add(1, Ordering::Relaxed);
                    return Ok(ModelLoadAdmission::Load(ModelLoadPermit {
                        coordinator: self,
                        key,
                        completed: false,
                    }));
                }
            }
        }
    }

    fn complete(&self, key: &(ModelId, BackendId), success: bool) -> AiResult<()> {
        let mut state = self.state.lock().map_err(|_| AiError::InternalState)?;
        if success {
            let last_used = state.next_use.saturating_add(1);
            state.next_use = last_used;
            state
                .routes
                .insert(key.clone(), LoadState::Ready { last_used });
        } else {
            state.routes.remove(key);
        }
        self.changed.notify_all();
        Ok(())
    }

    pub(crate) fn invalidate(&self, model: &ModelId, backend: &BackendId) -> AiResult<()> {
        let mut state = self.state.lock().map_err(|_| AiError::InternalState)?;
        if state
            .routes
            .remove(&(model.clone(), backend.clone()))
            .is_some()
        {
            self.invalidations.fetch_add(1, Ordering::Relaxed);
            self.changed.notify_all();
        }
        Ok(())
    }

    pub(crate) fn snapshot(&self) -> ModelLoadSnapshot {
        let Ok(state) = self.state.lock() else {
            return ModelLoadSnapshot::default();
        };
        ModelLoadSnapshot {
            ready: state
                .routes
                .values()
                .filter(|state| matches!(state, LoadState::Ready { .. }))
                .count(),
            loading: state
                .routes
                .values()
                .filter(|state| matches!(state, LoadState::Loading))
                .count(),
            ready_hits: self.ready_hits.load(Ordering::Relaxed),
            waiters: self.waiters.load(Ordering::Relaxed),
            loaders: self.loaders.load(Ordering::Relaxed),
            evictions: self.evictions.load(Ordering::Relaxed),
            invalidations: self.invalidations.load(Ordering::Relaxed),
        }
    }
}

pub(crate) enum ModelLoadAdmission<'a> {
    Ready,
    Load(ModelLoadPermit<'a>),
}

pub(crate) struct ModelLoadPermit<'a> {
    coordinator: &'a ModelLoadCoordinator,
    key: (ModelId, BackendId),
    completed: bool,
}

impl ModelLoadPermit<'_> {
    pub(crate) fn complete(mut self, success: bool) -> AiResult<()> {
        self.coordinator.complete(&self.key, success)?;
        self.completed = true;
        Ok(())
    }
}

impl Drop for ModelLoadPermit<'_> {
    fn drop(&mut self) {
        if !self.completed {
            let _ = self.coordinator.complete(&self.key, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    #[test]
    fn concurrent_waiter_observes_one_completed_route_load() {
        let coordinator = Arc::new(ModelLoadCoordinator::default());
        let model = ModelId::new("model/chat").unwrap();
        let backend = BackendId::new("backend/local").unwrap();
        let first = match coordinator
            .acquire(&model, &backend, None, &CancellationToken::new())
            .unwrap()
        {
            ModelLoadAdmission::Load(permit) => permit,
            ModelLoadAdmission::Ready => panic!("new route was unexpectedly ready"),
        };
        let barrier = Arc::new(Barrier::new(2));
        let waiting_coordinator = Arc::clone(&coordinator);
        let waiting_barrier = Arc::clone(&barrier);
        let waiting_model = model.clone();
        let waiting_backend = backend.clone();
        let waiter = std::thread::spawn(move || {
            waiting_barrier.wait();
            matches!(
                waiting_coordinator.acquire(
                    &waiting_model,
                    &waiting_backend,
                    Some(Duration::from_secs(1)),
                    &CancellationToken::new(),
                ),
                Ok(ModelLoadAdmission::Ready)
            )
        });
        barrier.wait();
        std::thread::yield_now();
        first.complete(true).unwrap();
        assert!(waiter.join().unwrap());
    }

    #[test]
    fn failed_load_reopens_route_for_retry() {
        let coordinator = ModelLoadCoordinator::default();
        let model = ModelId::new("model/retry").unwrap();
        let backend = BackendId::new("backend/retry").unwrap();
        let first = coordinator
            .acquire(&model, &backend, None, &CancellationToken::new())
            .unwrap();
        let ModelLoadAdmission::Load(first) = first else {
            panic!("new route was unexpectedly ready");
        };
        first.complete(false).unwrap();
        assert!(matches!(
            coordinator.acquire(&model, &backend, None, &CancellationToken::new()),
            Ok(ModelLoadAdmission::Load(_))
        ));
    }

    #[test]
    fn ready_routes_are_bounded_with_lru_eviction() {
        let coordinator = ModelLoadCoordinator::default();
        for index in 0..=MAX_TRACKED_ROUTES {
            let model = ModelId::new(format!("model/bounded-{index}")).unwrap();
            let backend = BackendId::new("backend/bounded").unwrap();
            let admission = coordinator
                .acquire(&model, &backend, None, &CancellationToken::new())
                .unwrap();
            let ModelLoadAdmission::Load(permit) = admission else {
                panic!("unique route was unexpectedly ready");
            };
            permit.complete(true).unwrap();
        }
        let snapshot = coordinator.snapshot();
        assert_eq!(snapshot.ready, MAX_TRACKED_ROUTES);
        assert_eq!(snapshot.evictions, 1);
    }
}
