// =============================================================================
//        #######
//     ###       ###     F: surface_state.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Surface-owned bounded reservations; guards remove cancelled or dropped futures.
use crate::{UiCancellation, UiConnectionState, UiEventV1, UiGrant};
use parking_lot::Mutex;
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};
pub(crate) const MAX_SAFE_SEQUENCE: u64 = 9_007_199_254_740_991;
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Owner {
    principal: String,
    tenant: appcore_core::TenantId,
    cluster: appcore_core::ClusterId,
    app: appcore_core::AppId,
    core: appcore_core::CoreId,
}
impl From<&UiGrant> for Owner {
    fn from(grant: &UiGrant) -> Self {
        Self {
            principal: grant.principal.clone(),
            tenant: grant.identity.tenant_id.clone(),
            cluster: grant.identity.cluster_id.clone(),
            app: grant.identity.runtime.app_id.clone(),
            core: grant.identity.core_id.clone(),
        }
    }
}
pub(crate) struct QueuedEvent {
    pub event: Arc<UiEventV1>,
    pub bytes: usize,
}
pub(crate) struct Subscription {
    pub capability: String,
    pub topic: String,
    pub owner: Owner,
    pub events: VecDeque<QueuedEvent>,
    pub dropped: u64,
}
pub(crate) struct State {
    pub connection: UiConnectionState,
    pub epoch: u64,
    pub flights: HashMap<String, UiCancellation>,
    pub subscriptions: HashMap<String, Subscription>,
    pub queued_bytes: usize,
    pub next_subscription: u64,
    pub next_event: u64,
    pub dropped_events: u64,
}
impl Default for State {
    fn default() -> Self {
        Self {
            connection: UiConnectionState::Connected,
            epoch: 0,
            flights: HashMap::new(),
            subscriptions: HashMap::new(),
            queued_bytes: 0,
            next_subscription: 0,
            next_event: 0,
            dropped_events: 0,
        }
    }
}
pub(crate) struct FlightGuard {
    pub state: Arc<Mutex<State>>,
    pub request_id: String,
    pub cancellation: UiCancellation,
}
impl Drop for FlightGuard {
    fn drop(&mut self) {
        self.cancellation.cancel();
        self.state.lock().flights.remove(&self.request_id);
    }
}
pub(crate) fn remove_subscription(state: &mut State, id: &str) {
    if let Some(subscription) = state.subscriptions.remove(id) {
        let bytes = subscription
            .events
            .iter()
            .map(|event| event.bytes)
            .sum::<usize>();
        state.queued_bytes = state.queued_bytes.saturating_sub(bytes);
    }
}
