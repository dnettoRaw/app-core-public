// =============================================================================
//        #######
//     ###       ###     F: backend.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Deployment-owned authentication and execution, separate from UI transport input.
use crate::{CommandRequest, QueryRequest, UiResult};
use appcore_capabilities::CapabilityRequest;
use appcore_contracts::ServiceId;
use appcore_core::{Clock, CoreIdentity, SystemClock};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
/// A caller identity constructed by the native/transport adapter, never decoded from UI JSON.
#[derive(Clone)]
pub struct UiCaller {
    pub(crate) surface_id: String,
    origin: String,
}
impl UiCaller {
    /// Binds a real transport caller to its configured surface and origin.
    pub fn new(surface_id: impl Into<String>, origin: impl Into<String>) -> UiResult<Self> {
        let surface_id = surface_id.into();
        crate::validation::validate_name(&surface_id)?;
        let origin = origin.into();
        if origin.is_empty() || origin.len() > 2048 {
            return Err(crate::UiError::InvalidRequest);
        }
        Ok(Self { surface_id, origin })
    }
    /// Returns the host-bound surface identity.
    pub fn surface_id(&self) -> &str {
        &self.surface_id
    }
    /// Returns the actual origin supplied by the adapter for authorization.
    pub fn origin(&self) -> &str {
        &self.origin
    }
}
/// Authenticated policy context produced by a deployment authorizer.
///
/// Principal must identify a session generation, not merely a reusable username.
/// The bridge checks capability/lease/write policy using this context after authentication.
pub struct UiGrant {
    pub(crate) principal: String,
    pub(crate) identity: CoreIdentity,
    pub(crate) service_id: ServiceId,
    pub(crate) writes_allowed: bool,
    pub(crate) now_ms: u64,
    pub(crate) leadership: Option<Arc<dyn appcore_distributed_contracts::ServiceLeadershipGuard>>,
}
impl UiGrant {
    /// Creates a read-only grant for an authenticated session generation.
    pub fn new(
        principal: impl Into<String>,
        identity: CoreIdentity,
        service_id: ServiceId,
    ) -> UiResult<Self> {
        let principal = principal.into();
        crate::validation::validate_name(&principal)?;
        Ok(Self {
            principal,
            identity,
            service_id,
            writes_allowed: false,
            now_ms: SystemClock.now_ms(),
            leadership: None,
        })
    }
    /// Applies the host's operational write policy explicitly.
    pub fn with_writes_allowed(mut self, allowed: bool) -> Self {
        self.writes_allowed = allowed;
        self
    }
    /// Supplies a service-scoped leadership guard; absent leadership fails closed when required.
    pub fn with_leadership(
        mut self,
        guard: Arc<dyn appcore_distributed_contracts::ServiceLeadershipGuard>,
    ) -> Self {
        self.leadership = Some(guard);
        self
    }
    /// Supplies the authorizer's trusted clock for lease validation.
    pub fn with_now_ms(mut self, now_ms: u64) -> Self {
        self.now_ms = now_ms;
        self
    }
}
/// Cooperative cancellation signal shared with a selected handler.
#[derive(Clone, Default)]
pub struct UiCancellation(Arc<AtomicBool>);
impl UiCancellation {
    /// Creates an uncancelled signal.
    pub fn new() -> Self {
        Self::default()
    }
    /// Requests cancellation; this never proves an application effect was rolled back.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    /// Reports whether the owner requested cancellation.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
/// Existing Runtime DTO selected for execution.
#[derive(Clone)]
pub enum UiInvocationV1 {
    /// A validated, side-effect-free query.
    Query(QueryRequest),
    /// A validated command with required idempotency.
    Command(CommandRequest),
}
/// Executor-independent owned/borrowed backend future.
pub type UiBackendFuture<'a> =
    Pin<Box<dyn Future<Output = UiResult<serde_json::Value>> + Send + 'a>>;
/// Mandatory deployment authentication/authorization and owning handler integration.
///
/// No allow-all default exists. Authenticate the actual caller, check permitted capability
/// scopes, and return its current session generation. Execution must delegate command
/// idempotency/effect recovery to the owning controller and must not block the executor.
/// Offloaded blocking work needs a separate bounded permit held until it actually ends.
pub trait UiBackend: Send + Sync {
    /// Authenticates the native caller and checks its capability scope.
    fn authorize(&self, caller: &UiCaller, request: &CapabilityRequest) -> UiResult<UiGrant>;
    /// Executes a validated and authorized operation without retaining a bridge lock.
    fn execute<'a>(
        &'a self,
        invocation: UiInvocationV1,
        cancellation: UiCancellation,
    ) -> UiBackendFuture<'a>;
}
