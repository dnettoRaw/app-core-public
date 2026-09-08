// =============================================================================
//        #######
//     ###       ###     F: reload_generation.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/02 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/02 12:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Bounded ownership and payload-free observations for HTTP routing generations.

use arc_swap::ArcSwap;
use axum::Router;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Weak};

pub(super) const MAX_RETAINED_GENERATIONS: usize = 2;

/// Payload-free state for one HTTP routing generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct HttpRoutingGenerationSnapshot {
    /// Monotonic identifier assigned by the composition root.
    pub generation: u64,
    /// Whether this generation can admit a new request.
    pub accepting: bool,
    /// Requests that currently retain this generation.
    pub inflight: usize,
}

/// Bounded ownership snapshot for active and retiring routing generations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct HttpRoutingGenerationsSnapshot {
    /// Generation selected for new requests.
    pub active: HttpRoutingGenerationSnapshot,
    /// Previous or failed generation retained only until its requests finish.
    pub retiring: Option<HttpRoutingGenerationSnapshot>,
    /// Number of generations retained by the reload owner.
    pub retained_generations: usize,
    /// Hard owner limit: one active generation and one retiring generation.
    pub max_retained_generations: usize,
}

pub(super) struct RoutingGeneration {
    id: u64,
    router: Router,
    accepting: AtomicBool,
    inflight: AtomicUsize,
    retiring_slot: Weak<RetiringGenerationSlot>,
}

impl RoutingGeneration {
    fn new(id: u64, router: Router, retiring_slot: Weak<RetiringGenerationSlot>) -> Self {
        Self {
            id,
            router,
            accepting: AtomicBool::new(true),
            inflight: AtomicUsize::new(0),
            retiring_slot,
        }
    }

    pub(super) fn id(&self) -> u64 {
        self.id
    }

    pub(super) fn router(&self) -> &Router {
        &self.router
    }

    pub(super) fn inflight(&self) -> usize {
        self.inflight.load(Ordering::Acquire)
    }

    pub(super) fn start_accepting(&self) {
        self.accepting.store(true, Ordering::Release);
    }

    pub(super) fn stop_accepting(&self) {
        self.accepting.store(false, Ordering::Release);
    }

    pub(super) fn try_admit(self: &Arc<Self>) -> Option<RoutingPermit> {
        if !self.accepting.load(Ordering::Acquire) {
            return None;
        }
        self.inflight
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current.checked_add(1)
            })
            .ok()?;
        if self.accepting.load(Ordering::Acquire) {
            return Some(RoutingPermit {
                generation: Arc::clone(self),
            });
        }
        self.release_request();
        None
    }

    fn release_request(&self) {
        let Ok(previous) =
            self.inflight
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                    current.checked_sub(1)
                })
        else {
            debug_assert!(false, "routing generation in-flight underflow");
            return;
        };
        if previous == 1 && !self.accepting.load(Ordering::Acquire) {
            if let Some(slot) = self.retiring_slot.upgrade() {
                slot.clear_if_drained(self.id);
            }
        }
    }

    fn snapshot(&self) -> HttpRoutingGenerationSnapshot {
        HttpRoutingGenerationSnapshot {
            generation: self.id,
            accepting: self.accepting.load(Ordering::Acquire),
            inflight: self.inflight(),
        }
    }
}

pub(super) struct RoutingPermit {
    generation: Arc<RoutingGeneration>,
}

impl Drop for RoutingPermit {
    fn drop(&mut self) {
        self.generation.release_request();
    }
}

struct RetiringGenerationSlot {
    generation: Mutex<Option<Arc<RoutingGeneration>>>,
}

impl RetiringGenerationSlot {
    fn new() -> Self {
        Self {
            generation: Mutex::new(None),
        }
    }

    fn retire(&self, generation: Arc<RoutingGeneration>) -> bool {
        let mut slot = self.generation.lock();
        if slot.is_some() {
            return false;
        }
        *slot = Some(generation);
        true
    }

    fn clear(&self, generation: u64) {
        let mut slot = self.generation.lock();
        if slot
            .as_ref()
            .is_some_and(|current| current.id == generation)
        {
            *slot = None;
        }
    }

    fn clear_if_drained(&self, generation: u64) {
        let mut slot = self.generation.lock();
        if slot
            .as_ref()
            .is_some_and(|current| current.id == generation && current.inflight() == 0)
        {
            *slot = None;
        }
    }

    fn snapshot(&self) -> Option<HttpRoutingGenerationSnapshot> {
        let slot = self.generation.lock();
        slot.as_ref().map(|generation| generation.snapshot())
    }

    fn clear_drained(&self) {
        let mut slot = self.generation.lock();
        if slot
            .as_ref()
            .is_some_and(|generation| generation.inflight() == 0)
        {
            *slot = None;
        }
    }
}

pub(super) struct RoutingTable {
    active: ArcSwap<RoutingGeneration>,
    retiring: Arc<RetiringGenerationSlot>,
}

impl RoutingTable {
    pub(super) fn new(id: u64, router: Router) -> Self {
        let retiring = Arc::new(RetiringGenerationSlot::new());
        let active = Arc::new(RoutingGeneration::new(
            id,
            router,
            Arc::downgrade(&retiring),
        ));
        Self {
            active: ArcSwap::from(active),
            retiring,
        }
    }

    pub(super) fn active(&self) -> Arc<RoutingGeneration> {
        self.active.load_full()
    }

    pub(super) fn generation(&self, id: u64, router: Router) -> Arc<RoutingGeneration> {
        Arc::new(RoutingGeneration::new(
            id,
            router,
            Arc::downgrade(&self.retiring),
        ))
    }

    pub(super) fn activate(&self, generation: Arc<RoutingGeneration>) {
        self.active.store(generation);
    }

    pub(super) fn retire(&self, generation: Arc<RoutingGeneration>) -> bool {
        self.retiring.retire(generation)
    }

    pub(super) fn release_retiring(&self, generation: u64) {
        self.retiring.clear(generation);
    }

    pub(super) fn release_drained_retiring(&self) {
        self.retiring.clear_drained();
    }

    pub(super) fn generations_snapshot(&self) -> HttpRoutingGenerationsSnapshot {
        let active = self.active();
        let active_snapshot = active.snapshot();
        let retiring = self
            .retiring
            .snapshot()
            .filter(|snapshot| snapshot.generation != active_snapshot.generation);
        HttpRoutingGenerationsSnapshot {
            active: active_snapshot,
            retiring,
            retained_generations: 1 + usize::from(retiring.is_some()),
            max_retained_generations: MAX_RETAINED_GENERATIONS,
        }
    }
}
