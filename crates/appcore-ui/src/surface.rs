// =============================================================================
//        #######
//     ###       ###     F: surface.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Engine-independent UI lifecycle, mandatory capability enforcement and async dispatch.
use crate::surface_state::{FlightGuard, State};
use crate::{
    UiActionV1, UiBackend, UiCaller, UiCancellation, UiConnectionState, UiError, UiGrant,
    UiInvocationV1, UiLimits, UiReplyV1, UiRequestV1, UiResult,
};
use appcore_capabilities::{CapabilityCatalog, CapabilityEnforcementContext, CapabilityRequest};
use appcore_core::{CapabilityMode, CapabilityName};
use futures_util::{
    future::{select, Either},
    FutureExt,
};
use parking_lot::Mutex;
use serde_json::Value;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
/// Explicit mapping from a notification topic to a declared read capability.
#[derive(Clone)]
pub struct UiTopicBinding {
    /// Topic published only by the trusted deployment producer.
    pub topic: String,
    /// Read capability checked before subscribe and every event-page read.
    pub capability: String,
}
/// Payload-free bounded operational snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiSurfaceSnapshot {
    /// Current link state.
    pub state: UiConnectionState,
    /// Futures executing or unwinding after cancellation.
    pub inflight: usize,
    /// Active subscriptions.
    pub subscriptions: usize,
    /// Accounted event bytes, including fanout into subscriber queues.
    pub queued_event_bytes: usize,
    /// Notifications rejected at full subscriber/byte boundaries.
    pub dropped_events: u64,
}
/// One deployment-created UI surface. It never starts a process, listener or engine.
///
/// Clones share admission and subscriptions. The adapter must bind its real caller to
/// the configured surface ID. Call close on native window loss and revoke on logout.
#[derive(Clone)]
pub struct UiSurface {
    pub(crate) id: String,
    pub(crate) origin: String,
    pub(crate) generation: String,
    pub(crate) limits: UiLimits,
    pub(crate) catalog: Arc<CapabilityCatalog>,
    pub(crate) topics: Arc<HashMap<String, String>>,
    pub(crate) backend: Arc<dyn UiBackend>,
    pub(crate) state: Arc<Mutex<State>>,
}
impl UiSurface {
    /// Deployment-selected immutable resource budgets.
    pub fn limits(&self) -> UiLimits {
        self.limits
    }
    /// Native host-generated surface identity.
    pub fn id(&self) -> &str {
        &self.id
    }
    /// Native origin selected by the deployment, never supplied by a wire request.
    pub fn origin(&self) -> &str {
        &self.origin
    }
    /// Creates one explicitly hosted surface with mandatory backend authorization.
    pub fn new(
        caller: UiCaller,
        limits: UiLimits,
        catalog: CapabilityCatalog,
        topics: impl IntoIterator<Item = UiTopicBinding>,
        backend: Arc<dyn UiBackend>,
    ) -> UiResult<Self> {
        let id = caller.surface_id().to_string();
        let origin = caller.origin().to_string();
        crate::validation::validate_name(&id)?;
        let limits = limits.validate()?;
        if limits.max_frame_bytes < limits.max_payload_bytes.saturating_add(1024) {
            return Err(UiError::InvalidRequest);
        }
        let mut bindings = HashMap::new();
        for binding in topics {
            crate::validation::validate_name(&binding.topic)?;
            crate::validation::validate_name(&binding.capability)?;
            let name = CapabilityName::new(binding.capability.clone())
                .map_err(|_| UiError::InvalidRequest)?;
            if catalog
                .descriptor(&name)
                .is_none_or(|descriptor| descriptor.mode != CapabilityMode::Query)
            {
                return Err(UiError::Forbidden);
            }
            if bindings.len() >= 1024
                || bindings.insert(binding.topic, binding.capability).is_some()
            {
                return Err(UiError::InvalidRequest);
            }
        }
        let mut nonce = [0u8; 16];
        getrandom::fill(&mut nonce).map_err(|_| UiError::Unavailable)?;
        let generation = nonce
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        Ok(Self {
            id,
            origin,
            generation,
            limits,
            catalog: Arc::new(catalog),
            topics: Arc::new(bindings),
            backend,
            state: Arc::new(Mutex::new(State::default())),
        })
    }
    /// Decodes a bounded request and returns a bounded correlated JSON reply.
    pub async fn dispatch_json(&self, caller: &UiCaller, bytes: &[u8]) -> UiResult<String> {
        let request = crate::decode_ui_request_v1(bytes, self.limits)?;
        let reply = self.dispatch(caller, request).await?;
        crate::json_budget::json_bytes(&reply, self.limits.max_frame_bytes)?;
        serde_json::to_string(&reply).map_err(|_| UiError::InvalidFrame)
    }
    /// Executes a validated request; valid operation failures become controlled replies.
    pub async fn dispatch(&self, caller: &UiCaller, request: UiRequestV1) -> UiResult<UiReplyV1> {
        request.validate(self.limits)?;
        if caller.surface_id() != self.id
            || caller.origin() != self.origin
            || request.surface_id != self.id
        {
            return Err(UiError::Forbidden);
        }
        let result = self.handle(caller, &request).await;
        let reply = UiReplyV1::new(
            &self.id,
            &request.request_id,
            self.state.lock().connection,
            result,
        );
        if crate::json_budget::json_bytes(&reply, self.limits.max_frame_bytes).is_err() {
            return Ok(UiReplyV1::new(
                &self.id,
                &request.request_id,
                self.state.lock().connection,
                Err(UiError::PayloadTooLarge),
            ));
        }
        Ok(reply)
    }
    async fn handle(&self, caller: &UiCaller, request: &UiRequestV1) -> UiResult<Value> {
        if let UiActionV1::Close = request.action {
            self.close();
            return Ok(Value::Null);
        }
        if self.state.lock().connection == UiConnectionState::Closed {
            return Err(UiError::Closed);
        }
        match &request.action {
            UiActionV1::Query { query } => {
                self.invoke(caller, request, UiInvocationV1::Query(query.clone()))
                    .await
            }
            UiActionV1::Command { command } => {
                self.invoke(caller, request, UiInvocationV1::Command(command.clone()))
                    .await
            }
            UiActionV1::Subscribe { capability, topic } => {
                self.subscribe(caller, request, capability, topic)
            }
            UiActionV1::Poll { subscription_id } => self.poll(caller, request, subscription_id),
            UiActionV1::Unsubscribe { subscription_id } => {
                crate::surface_state::remove_subscription(&mut self.state.lock(), subscription_id);
                Ok(Value::Null)
            }
            UiActionV1::Cancel { target_request_id } => {
                let state = self.state.lock();
                let present = state.flights.get(target_request_id);
                if let Some(signal) = present {
                    signal.cancel();
                }
                Ok(Value::Bool(present.is_some()))
            }
            UiActionV1::Close => Ok(Value::Null),
        }
    }
    pub(crate) fn authorize(
        &self,
        caller: &UiCaller,
        request: &CapabilityRequest,
    ) -> UiResult<UiGrant> {
        self.catalog
            .resolve_local(request)
            .map_err(|_| UiError::Forbidden)?;
        let grant = catch_unwind(AssertUnwindSafe(|| self.backend.authorize(caller, request)))
            .map_err(|_| UiError::BackendFailure)?
            .inspect_err(|error| {
                if matches!(error, UiError::AuthenticationRequired) {
                    self.revoke();
                }
            })?;
        let mut context =
            CapabilityEnforcementContext::new(&grant.identity, &grant.service_id, grant.now_ms)
                .with_writes_allowed(grant.writes_allowed);
        if let Some(guard) = &grant.leadership {
            context = context.with_leadership(guard.as_ref());
        }
        self.catalog
            .authorize_local(request, context)
            .map_err(|_| UiError::Forbidden)?;
        Ok(grant)
    }
    async fn invoke(
        &self,
        caller: &UiCaller,
        frame: &UiRequestV1,
        invocation: UiInvocationV1,
    ) -> UiResult<Value> {
        let started = Instant::now();
        let epoch = self.state.lock().epoch;
        let timeout = Duration::from_millis(u64::from(frame.timeout_ms));
        let capability = match &invocation {
            UiInvocationV1::Query(query) => CapabilityRequest {
                request_id: query.query_id.clone(),
                capability: CapabilityName::new(query.query_name.clone())
                    .map_err(|_| UiError::InvalidRequest)?,
                mode: CapabilityMode::Query,
                payload: query.payload_bytes(),
                idempotency_key: None,
                trace: None,
            },
            UiInvocationV1::Command(command) => CapabilityRequest {
                request_id: command.command_id.clone(),
                capability: CapabilityName::new(command.command_name.clone())
                    .map_err(|_| UiError::InvalidRequest)?,
                mode: CapabilityMode::Command,
                payload: command.payload_bytes().to_vec(),
                idempotency_key: command.idempotency_key.clone(),
                trace: None,
            },
        };
        let _grant = self.authorize(caller, &capability)?;
        if started.elapsed() >= timeout {
            return Err(UiError::DeadlineExceeded);
        }
        let cancellation = UiCancellation::new();
        {
            let mut state = self.state.lock();
            if state.epoch != epoch {
                return Err(UiError::Cancelled);
            }
            match state.connection {
                UiConnectionState::Closed => return Err(UiError::Closed),
                UiConnectionState::Offline => return Err(UiError::Unavailable),
                _ => {}
            }
            if state.flights.len() >= self.limits.max_inflight
                || state.flights.contains_key(&frame.request_id)
            {
                return Err(UiError::Busy);
            }
            state
                .flights
                .insert(frame.request_id.clone(), cancellation.clone());
        }
        let _flight = FlightGuard {
            state: Arc::clone(&self.state),
            request_id: frame.request_id.clone(),
            cancellation: cancellation.clone(),
        };
        let work = catch_unwind(AssertUnwindSafe(|| {
            self.backend.execute(invocation, cancellation.clone())
        }))
        .map_err(|_| UiError::BackendFailure)?;
        let work = Box::pin(AssertUnwindSafe(work).catch_unwind());
        if cancellation.is_cancelled() {
            return Err(UiError::Cancelled);
        }
        if started.elapsed() >= timeout {
            return Err(UiError::DeadlineExceeded);
        }
        let stop = Box::pin(wait_stop(
            cancellation.clone(),
            timeout.saturating_sub(started.elapsed()),
        ));
        match select(work, stop).await {
            Either::Left((result, _)) => {
                if cancellation.is_cancelled() {
                    return Err(UiError::Cancelled);
                }
                if started.elapsed() >= timeout {
                    return Err(UiError::DeadlineExceeded);
                }
                let value = result.map_err(|_| UiError::BackendFailure)??;
                crate::json_budget::json_bytes(&value, self.limits.max_payload_bytes)?;
                Ok(value)
            }
            Either::Right((error, _)) => {
                cancellation.cancel();
                Err(error)
            }
        }
    }
    /// Closes admission, signals cancellation and releases all queued UI notifications.
    /// Owned callers must still poll/join their futures; effects are not rolled back.
    pub fn close(&self) {
        let mut state = self.state.lock();
        state.connection = UiConnectionState::Closed;
        state.epoch = state.epoch.saturating_add(1);
        for signal in state.flights.values() {
            signal.cancel();
        }
        state.subscriptions.clear();
        state.queued_bytes = 0;
    }
    /// Revokes current session resources without making a replacement session implicit.
    pub fn revoke(&self) {
        let mut state = self.state.lock();
        if let Some(epoch) = state.epoch.checked_add(1) {
            state.epoch = epoch;
        } else {
            state.connection = UiConnectionState::Closed;
        }
        for signal in state.flights.values() {
            signal.cancel();
        }
        state.subscriptions.clear();
        state.queued_bytes = 0;
    }
    /// Updates link health. A closed surface can only be replaced, never reopened.
    pub fn set_connection_state(&self, connection: UiConnectionState) -> UiResult<()> {
        if connection == UiConnectionState::Closed {
            self.close();
            return Ok(());
        }
        let mut state = self.state.lock();
        if state.connection == UiConnectionState::Closed {
            return Err(UiError::Closed);
        }
        state.connection = connection;
        if connection == UiConnectionState::Offline {
            if let Some(epoch) = state.epoch.checked_add(1) {
                state.epoch = epoch;
            } else {
                state.connection = UiConnectionState::Closed;
            }
            for signal in state.flights.values() {
                signal.cancel();
            }
            state.subscriptions.clear();
            state.queued_bytes = 0;
        }
        Ok(())
    }
    /// Returns bounded payload-free health/admission observations.
    pub fn snapshot(&self) -> UiSurfaceSnapshot {
        let state = self.state.lock();
        UiSurfaceSnapshot {
            state: state.connection,
            inflight: state.flights.len(),
            subscriptions: state.subscriptions.len(),
            queued_event_bytes: state.queued_bytes,
            dropped_events: state.dropped_events,
        }
    }
}
async fn wait_stop(cancellation: UiCancellation, timeout: Duration) -> UiError {
    let started = Instant::now();
    loop {
        if cancellation.is_cancelled() {
            return UiError::Cancelled;
        }
        if started.elapsed() >= timeout {
            return UiError::DeadlineExceeded;
        }
        futures_timer::Delay::new(
            timeout
                .saturating_sub(started.elapsed())
                .min(Duration::from_millis(5)),
        )
        .await;
    }
}
