// =============================================================================
//        #######
//     ###       ###     F: application_ai.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-alpha
// =============================================================================

//! Opt-in post-1.0 composition adapter for `appcore-ai`.
//!
//! This module deliberately does not add fields to frozen V1 manifests. A
//! future versioned contract may select it declaratively after architecture
//! acceptance. Until then, callers compose the component explicitly.

use crate::bootstrap::BootstrapError;
use appcore_ai::{AiError, AiRequest, AiResponse, AiRuntime, AiStreamSink, CancellationToken};
use appcore_capabilities::{
    CapabilityError, CapabilityRegistry, CapabilityRequest, CapabilityResponse, CapabilityResult,
    LocalCapabilityHandler,
};
use appcore_core::{
    CapabilityDescriptor, CapabilityMode, CapabilityName, CapabilityRequirements,
    CapabilityVisibility,
};
use appcore_supervisor::{
    CallbackManagedService, DependencyRequirement, ManagedResource, ManagedService, RestartPolicy,
    ServiceDescriptor, ServiceHealth,
};
use std::collections::BTreeMap;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Context, Poll, Wake, Waker};
use std::time::{Duration, Instant};

/// Stable local capability name used by the alpha composition adapter.
pub const AI_RESOLVE_CAPABILITY: &str = "appcore.ai.resolve";
const MAX_CAPABILITY_PAYLOAD_BYTES: usize = 4 * 1_024 * 1_024;

/// Versioned payload codec owned by a composition or application boundary.
///
/// `appcore-ai` intentionally does not make its Rust API an implicit wire
/// format. A codec must bound and validate bytes before producing `AiRequest`.
pub trait AiCapabilityCodec: Send + Sync {
    /// Decodes one bounded capability payload.
    fn decode_request(&self, payload: &[u8]) -> Result<AiRequest, String>;

    /// Encodes one validated response into bounded capability bytes.
    fn encode_response(&self, response: &AiResponse) -> Result<Vec<u8>, String>;
}

#[derive(Debug)]
struct ActiveState {
    accepting: bool,
    active: BTreeMap<u64, CancellationToken>,
}

struct SharedAi {
    runtime: Arc<AiRuntime>,
    state: Mutex<ActiveState>,
    drained: Condvar,
    next_request: AtomicU64,
    required: bool,
}

/// Cloneable application-facing facade for bounded AI resolution.
#[derive(Clone)]
pub struct ApplicationAi {
    shared: Arc<SharedAi>,
}

impl std::fmt::Debug for ApplicationAi {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ApplicationAi")
            .field("health", &self.shared.runtime.health().ok())
            .finish_non_exhaustive()
    }
}

impl ApplicationAi {
    /// Resolves with a facade-owned cancellation token tracked for shutdown.
    pub async fn resolve(&self, request: AiRequest) -> Result<AiResponse, AiError> {
        self.resolve_with_cancellation(request, CancellationToken::new())
            .await
    }

    /// Resolves with caller-owned cancellation also tracked by graceful shutdown.
    pub async fn resolve_with_cancellation(
        &self,
        request: AiRequest,
        cancellation: CancellationToken,
    ) -> Result<AiResponse, AiError> {
        let _guard = self.track(cancellation.clone())?;
        self.shared
            .runtime
            .resolve_with_cancellation(request, cancellation)
            .await
    }

    /// Resolves a stream with facade-owned cancellation tracked for shutdown.
    pub async fn resolve_stream(
        &self,
        request: AiRequest,
        sink: &dyn AiStreamSink,
    ) -> Result<AiResponse, AiError> {
        self.resolve_stream_with_cancellation(request, CancellationToken::new(), sink)
            .await
    }

    /// Resolves a stream with caller-owned cancellation tracked for shutdown.
    pub async fn resolve_stream_with_cancellation(
        &self,
        request: AiRequest,
        cancellation: CancellationToken,
        sink: &dyn AiStreamSink,
    ) -> Result<AiResponse, AiError> {
        let _guard = self.track(cancellation.clone())?;
        self.shared
            .runtime
            .resolve_stream_with_cancellation(request, cancellation, sink)
            .await
    }

    fn track(&self, cancellation: CancellationToken) -> Result<ActiveRequest, AiError> {
        let sequence = self.shared.next_request.fetch_add(1, Ordering::Relaxed);
        {
            let mut state = self
                .shared
                .state
                .lock()
                .map_err(|_| AiError::InternalState)?;
            if !state.accepting {
                return Err(AiError::BackendUnavailable(appcore_ai::BackendId::new(
                    "appcore/ai-component",
                )?));
            }
            state.active.insert(sequence, cancellation.clone());
        }
        let guard = ActiveRequest {
            shared: Arc::clone(&self.shared),
            sequence,
        };
        Ok(guard)
    }

    /// Returns the aggregate runtime health used by the managed service.
    pub fn health(&self) -> Result<appcore_ai::AiRuntimeHealth, AiError> {
        self.shared.runtime.health()
    }

    fn start(&self) -> Result<(), String> {
        let health = self
            .shared
            .runtime
            .health()
            .map_err(|_| "AI runtime health unavailable".to_string())?;
        if self.shared.required && !health.is_available() {
            return Err("required AI runtime has no usable backend/model".to_string());
        }
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| "AI lifecycle state unavailable".to_string())?;
        state.accepting = true;
        Ok(())
    }

    fn shutdown(&self, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| "AI lifecycle state unavailable".to_string())?;
        state.accepting = false;
        for token in state.active.values() {
            token.cancel();
        }
        while !state.active.is_empty() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("AI graceful shutdown timed out".to_string());
            }
            let waited = self
                .shared
                .drained
                .wait_timeout(state, remaining)
                .map_err(|_| "AI lifecycle state unavailable".to_string())?;
            state = waited.0;
            if waited.1.timed_out() && !state.active.is_empty() {
                return Err("AI graceful shutdown timed out".to_string());
            }
        }
        Ok(())
    }

    fn service_health(&self) -> ServiceHealth {
        let accepting = self
            .shared
            .state
            .lock()
            .map(|state| state.accepting)
            .unwrap_or(false);
        if !accepting {
            return ServiceHealth::Unknown;
        }
        match self.shared.runtime.health() {
            Ok(health) if health.is_available() => ServiceHealth::Healthy,
            Ok(_) if !self.shared.required => ServiceHealth::Degraded,
            Ok(_) | Err(_) => ServiceHealth::Failed,
        }
    }
}

struct ActiveRequest {
    shared: Arc<SharedAi>,
    sequence: u64,
}

impl Drop for ActiveRequest {
    fn drop(&mut self) {
        let Ok(mut state) = self.shared.state.lock() else {
            return;
        };
        state.active.remove(&self.sequence);
        if state.active.is_empty() {
            self.shared.drained.notify_all();
        }
    }
}

/// Opt-in AI component registered into the existing AppCore Supervisor.
pub struct AppCoreAiComponent {
    facade: ApplicationAi,
    service: Arc<CallbackManagedService>,
}

impl std::fmt::Debug for AppCoreAiComponent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AppCoreAiComponent")
            .field("service", &self.service.descriptor().name())
            .field("health", &self.service.health())
            .finish_non_exhaustive()
    }
}

impl AppCoreAiComponent {
    /// Composes lifecycle around an already configured backend-neutral runtime.
    pub fn new(runtime: Arc<AiRuntime>, required: bool) -> Result<Self, BootstrapError> {
        let facade = ApplicationAi {
            shared: Arc::new(SharedAi {
                runtime,
                state: Mutex::new(ActiveState {
                    accepting: false,
                    active: BTreeMap::new(),
                }),
                drained: Condvar::new(),
                next_request: AtomicU64::new(1),
                required,
            }),
        };
        let policy = RestartPolicy::bounded(3, Duration::from_secs(600))
            .map_err(supervisor_error)?
            .with_shutdown_timeout(Duration::from_secs(30));
        let descriptor = ServiceDescriptor::new("appcore-ai", ManagedResource::Worker, policy)
            .map_err(supervisor_error)?
            .with_dependency_requirement("runtime", DependencyRequirement::Ready)
            .map_err(supervisor_error)?
            .with_dependency_requirement("security", DependencyRequirement::Healthy)
            .map_err(supervisor_error)?
            .with_critical(required);
        let start_facade = facade.clone();
        let stop_facade = facade.clone();
        let health_facade = facade.clone();
        let service = Arc::new(CallbackManagedService::new(
            descriptor,
            move || start_facade.start(),
            move |timeout| stop_facade.shutdown(timeout),
            move || health_facade.service_health(),
        ));
        Ok(Self { facade, service })
    }

    /// Returns the application-facing facade.
    #[must_use]
    pub fn facade(&self) -> ApplicationAi {
        self.facade.clone()
    }

    /// Returns the managed service registered into the existing Supervisor.
    #[must_use]
    pub fn managed_service(&self) -> Arc<dyn ManagedService> {
        self.service.clone()
    }

    /// Registers the local resolve handler in the canonical capability registry.
    pub fn register_capability(
        &self,
        registry: &mut CapabilityRegistry,
        codec: Arc<dyn AiCapabilityCodec>,
    ) -> CapabilityResult<()> {
        let descriptor = ai_capability_descriptor()?;
        registry.register_handler(AiCapabilityHandler {
            facade: self.facade.clone(),
            service: self.service.clone(),
            codec,
            descriptor,
        })
    }
}

struct AiCapabilityHandler {
    facade: ApplicationAi,
    service: Arc<CallbackManagedService>,
    codec: Arc<dyn AiCapabilityCodec>,
    descriptor: CapabilityDescriptor,
}

impl LocalCapabilityHandler for AiCapabilityHandler {
    fn descriptor(&self) -> CapabilityDescriptor {
        self.descriptor.clone()
    }

    fn is_healthy(&self) -> bool {
        matches!(
            self.service.health(),
            ServiceHealth::Ready | ServiceHealth::Healthy | ServiceHealth::Degraded
        )
    }

    fn handle(&self, request: &CapabilityRequest) -> CapabilityResult<CapabilityResponse> {
        if request.capability.as_str() != AI_RESOLVE_CAPABILITY
            || request.mode != CapabilityMode::Query
            || request.payload.len() > MAX_CAPABILITY_PAYLOAD_BYTES
        {
            return Err(CapabilityError::HandlerRejected(
                "invalid AI capability request".to_string(),
            ));
        }
        let ai_request = self.codec.decode_request(&request.payload).map_err(|_| {
            CapabilityError::HandlerRejected("invalid AI capability payload".to_string())
        })?;
        let response = block_on(self.facade.resolve(ai_request))
            .map_err(|_| CapabilityError::HandlerRejected("AI resolution failed".to_string()))?;
        let payload = self.codec.encode_response(&response).map_err(|_| {
            CapabilityError::HandlerRejected("AI response encoding failed".to_string())
        })?;
        if payload.len() > MAX_CAPABILITY_PAYLOAD_BYTES {
            return Err(CapabilityError::HandlerRejected(
                "AI capability response is too large".to_string(),
            ));
        }
        Ok(CapabilityResponse::accepted(payload, None))
    }
}

fn ai_capability_descriptor() -> CapabilityResult<CapabilityDescriptor> {
    let name = CapabilityName::new(AI_RESOLVE_CAPABILITY).map_err(|_| {
        CapabilityError::HandlerRejected("invalid AI capability descriptor".to_string())
    })?;
    Ok(CapabilityDescriptor::new(
        name,
        "0.1.0-alpha",
        CapabilityMode::Query,
        CapabilityVisibility::Local,
    )
    .with_requirements(CapabilityRequirements {
        requires_leader: false,
        read_only: true,
        idempotency_required: false,
    }))
}

fn block_on<T>(future: impl Future<Output = T>) -> T {
    struct ThreadWake(std::thread::Thread);
    impl Wake for ThreadWake {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(ThreadWake(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::park_timeout(Duration::from_millis(10)),
        }
    }
}

fn supervisor_error(error: appcore_supervisor::SupervisorError) -> BootstrapError {
    BootstrapError::Runtime(format!("AI supervisor configuration failed: {error}"))
}

#[cfg(test)]
#[path = "application_ai_tests.rs"]
mod tests;
