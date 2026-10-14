// =============================================================================
//        #######
//     ###       ###     F: runtime_owner.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

// appcore-norm: test

use appcore_sdk::{App, Application};
use appcore_ui::{capabilities::CapabilityCatalog, contracts::ServiceId, core::*, *};
use serde_json::json;
use std::{
    future::Future,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll, Waker},
};
struct ApplicationState(Arc<AtomicUsize>);
struct ChangeHandler(Arc<AtomicUsize>);
impl CommandHandler for ChangeHandler {
    fn command_name(&self) -> CommandName {
        CommandName::new("state.change").unwrap()
    }
    fn handle(
        &self,
        command: &CommandEnvelope,
        context: &dyn RuntimeContext,
    ) -> RuntimeResult<CommandResult> {
        let revision = self.0.fetch_add(1, Ordering::SeqCst) + 1;
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
    fn register_handlers(&self, bus: &mut CommandBus) -> RuntimeResult<()> {
        bus.register_handler(ChangeHandler(self.0.clone()))
    }
}
struct ContextOwner(RuntimeIdentity);
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
struct OwnerBackend {
    dispatch: UiRuntimeDispatch,
    identity: CoreIdentity,
}
impl UiBackend for OwnerBackend {
    fn authorize(
        &self,
        caller: &UiCaller,
        _: &capabilities::CapabilityRequest,
    ) -> UiResult<UiGrant> {
        if caller.origin() != "app://local" {
            return Err(UiError::Forbidden);
        }
        UiGrant::new(
            "verified-fixture-session",
            self.identity.clone(),
            ServiceId::new("service-a").unwrap(),
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
fn run<T>(future: impl Future<Output = T>) -> T {
    let mut future = Box::pin(future);
    let mut context = Context::from_waker(Waker::noop());
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}
#[test]
fn sdk_prepared_application_keeps_effect_receipt_and_event_idempotency_ownership() {
    let effects = Arc::new(AtomicUsize::new(0));
    let app = App::new("surface-consumer").unwrap();
    let prepared = app
        .prepare(
            &ApplicationState(effects.clone()),
            NodeId::new("node-a").unwrap(),
        )
        .unwrap();
    let parts = prepared.into_parts();
    let runtime = parts.0;
    let queries = parts.1;
    let identity = CoreIdentity {
        tenant_id: TenantId::new("tenant-a").unwrap(),
        cluster_id: ClusterId::new("cluster-a").unwrap(),
        core_id: CoreId::new("core-a").unwrap(),
        instance_id: InstanceId::new("instance-a").unwrap(),
        kind: CoreKind::operational(),
        protocol_version: ProtocolVersion::new(1),
        runtime: runtime.identity().clone(),
    };
    let context = Arc::new(ContextOwner(runtime.identity().clone()));
    let controller = RuntimeController::new(runtime);
    for event in [
        RuntimeLifecycleEvent::ConfigLoaded,
        RuntimeLifecycleEvent::SecurityChecked,
        RuntimeLifecycleEvent::StorageOpened,
        RuntimeLifecycleEvent::ApiStarted,
    ] {
        controller.apply_lifecycle_event(event).unwrap();
    }
    let dispatch =
        UiRuntimeDispatch::new(controller.clone(), queries, context, UiLimits::default()).unwrap();
    let backend = Arc::new(OwnerBackend { dispatch, identity });
    let descriptor = CapabilityDescriptor {
        name: CapabilityName::new("state.change").unwrap(),
        version: "1".into(),
        mode: CapabilityMode::Command,
        visibility: CapabilityVisibility::Local,
        requirements: CapabilityRequirements {
            idempotency_required: true,
            ..Default::default()
        },
    };
    let catalog = CapabilityCatalog::from_descriptors([descriptor]).unwrap();
    let caller = UiCaller::new("surface-a", "app://local").unwrap();
    let surface =
        UiSurface::new(caller.clone(), UiLimits::default(), catalog, [], backend).unwrap();
    let mut results = Vec::new();
    for id in ["request-a", "request-b"] {
        let frame = UiRequestV1 {
            version: 1,
            surface_id: "surface-a".into(),
            request_id: id.into(),
            timeout_ms: 1000,
            action: UiActionV1::Command {
                command: CommandRequest {
                    command_name: "state.change".into(),
                    command_id: id.into(),
                    idempotency_key: Some("one-operation".into()),
                    payload: "{}".into(),
                },
            },
        };
        let reply = run(surface.dispatch(&caller, frame)).unwrap();
        match reply.outcome {
            UiOutcomeV1::Ok { value } => results.push(value),
            _ => panic!("command failed"),
        }
    }
    assert_eq!(results[0], results[1]);
    assert_eq!(effects.load(Ordering::SeqCst), 1);
    assert_eq!(controller.instance().event_bus().len(), 1);
    let conflict = UiRequestV1 {
        version: 1,
        surface_id: "surface-a".into(),
        request_id: "conflict".into(),
        timeout_ms: 1000,
        action: UiActionV1::Command {
            command: CommandRequest {
                command_name: "state.change".into(),
                command_id: "conflict".into(),
                idempotency_key: Some("one-operation".into()),
                payload: json!({"different":true}).to_string(),
            },
        },
    };
    assert!(matches!(
        run(surface.dispatch(&caller, conflict)).unwrap().outcome,
        UiOutcomeV1::Error {
            code: UiError::IdempotencyConflict
        }
    ));
    assert_eq!(effects.load(Ordering::SeqCst), 1);
    assert_eq!(controller.instance().event_bus().len(), 1);
}
