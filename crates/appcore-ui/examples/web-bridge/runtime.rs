// =============================================================================
//        #######
//     ###       ###     F: runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Nonblocking SDK fixture with one bounded state effect and explicit UI authorization.

use appcore_sdk::Application;
use appcore_ui::{contracts::ServiceId, core::*, *};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
pub struct ApplicationState(pub Arc<AtomicUsize>);
struct ChangeHandler {
    name: CommandName,
    state: Arc<AtomicUsize>,
}
impl CommandHandler for ChangeHandler {
    fn command_name(&self) -> CommandName {
        self.name.clone()
    }
    fn handle(
        &self,
        command: &CommandEnvelope,
        context: &dyn RuntimeContext,
    ) -> RuntimeResult<CommandResult> {
        let revision = self.state.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(CommandResult::accepted(vec![EventEnvelope::new(
            EventName::new("state.changed")?,
            format!("event-{revision}"),
            context.app_id().clone(),
            context.node_id().clone(),
            command.issued_at_ms,
            vec![],
        )?]))
    }
}
impl Application for ApplicationState {
    fn register_queries(&self, router: &mut appcore_api::ApiRouter) -> RuntimeResult<()> {
        router.register_query(Inspect {
            name: QueryName::new("state.inspect")?,
            state: self.0.clone(),
        })
    }
    fn register_handlers(&self, bus: &mut CommandBus) -> RuntimeResult<()> {
        bus.register_handler(ChangeHandler {
            name: CommandName::new("state.change")?,
            state: self.0.clone(),
        })
    }
}
pub struct ContextOwner(pub RuntimeIdentity);
impl RuntimeContext for ContextOwner {
    fn app_id(&self) -> &AppId {
        &self.0.app_id
    }
    fn app_family(&self) -> &AppFamily {
        &self.0.app_family
    }
    fn sync_group(&self) -> &SyncGroup {
        &self.0.sync_group
    }
    fn runtime_contract(&self) -> RuntimeContractVersion {
        self.0.runtime_contract
    }
    fn node_id(&self) -> &NodeId {
        &self.0.node_id
    }
}
pub struct OwnerBackend {
    pub dispatch: UiRuntimeDispatch,
    pub identity: CoreIdentity,
}
impl UiBackend for OwnerBackend {
    fn authorize(
        &self,
        caller: &UiCaller,
        _: &capabilities::CapabilityRequest,
    ) -> UiResult<UiGrant> {
        if !caller.origin().starts_with("appcore-ui://") {
            return Err(UiError::Forbidden);
        }
        UiGrant::new(
            "verified-fixture-session",
            self.identity.clone(),
            ServiceId::new("service-a").map_err(|_| UiError::InvalidRequest)?,
        )
        .map(|grant| grant.with_writes_allowed(true))
    }
    fn execute<'a>(
        &'a self,
        invocation: UiInvocationV1,
        cancellation: UiCancellation,
    ) -> UiBackendFuture<'a> {
        // The controlled fixture handler performs only bounded atomic work. Real blocking
        // handlers must be offloaded by the deployment with an actual-worker permit.
        Box::pin(async move { self.dispatch.invoke(invocation, &cancellation) })
    }
}

struct Inspect {
    name: QueryName,
    state: Arc<AtomicUsize>,
}
impl appcore_api::QueryEndpoint for Inspect {
    fn query_name(&self) -> &QueryName {
        &self.name
    }
    fn handle_query(
        &self,
        request: appcore_api::ApiRequest,
    ) -> RuntimeResult<appcore_api::ApiResponse> {
        if request.payload != b"null" {
            return Ok(appcore_api::ApiResponse {
                status_code: 400,
                payload: vec![],
            });
        }
        Ok(appcore_api::ApiResponse {
            status_code: 200,
            payload: format!("{{\"effects\":{}}}", self.state.load(Ordering::SeqCst)).into_bytes(),
        })
    }
}

pub fn surface(caller: UiCaller, backend: Arc<OwnerBackend>) -> UiResult<UiSurface> {
    use appcore_ui::capabilities::CapabilityCatalog;
    let descriptors = [
        ("state.inspect", CapabilityMode::Query),
        ("state.change", CapabilityMode::Command),
    ]
    .into_iter()
    .map(|(name, mode)| {
        Ok(CapabilityDescriptor {
            name: CapabilityName::new(name).map_err(|_| UiError::InvalidRequest)?,
            version: "1".into(),
            mode,
            visibility: CapabilityVisibility::Local,
            requirements: CapabilityRequirements {
                requires_leader: false,
                read_only: mode == CapabilityMode::Query,
                idempotency_required: mode == CapabilityMode::Command,
            },
        })
    })
    .collect::<UiResult<Vec<_>>>()?;
    UiSurface::new(
        caller,
        UiLimits::default(),
        CapabilityCatalog::from_descriptors(descriptors).map_err(|_| UiError::InvalidRequest)?,
        [UiTopicBinding {
            topic: "state.changed".into(),
            capability: "state.inspect".into(),
        }],
        backend,
    )
}
