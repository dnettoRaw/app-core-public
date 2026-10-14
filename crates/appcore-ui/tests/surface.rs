// =============================================================================
//        #######
//     ###       ###     F: surface.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

// appcore-norm: test

use appcore_ui::{
    capabilities::{CapabilityCatalog, CapabilityRequest},
    contracts::ServiceId,
    core::*,
    *,
};
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::{
    future::Future,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll, Waker},
    time::Duration,
};
fn identity() -> CoreIdentity {
    CoreIdentity {
        tenant_id: TenantId::new("tenant-a").unwrap(),
        cluster_id: ClusterId::new("cluster-a").unwrap(),
        core_id: CoreId::new("core-a").unwrap(),
        instance_id: InstanceId::new("instance-a").unwrap(),
        kind: CoreKind::operational(),
        protocol_version: ProtocolVersion::new(1),
        runtime: RuntimeIdentity {
            app_id: AppId::new("application-a").unwrap(),
            app_family: AppFamily::new("family-a").unwrap(),
            sync_group: SyncGroup::new("group-a").unwrap(),
            runtime_contract: RuntimeContractVersion::new(1),
            node_id: NodeId::new("node-a").unwrap(),
        },
    }
}
struct Backend {
    authenticated: AtomicBool,
    allowed: AtomicBool,
    writes: AtomicBool,
    pending: AtomicBool,
    calls: AtomicUsize,
    principal: Mutex<String>,
    last_signal: Mutex<Option<UiCancellation>>,
}
impl Default for Backend {
    fn default() -> Self {
        Self {
            authenticated: AtomicBool::new(true),
            allowed: AtomicBool::new(true),
            writes: AtomicBool::new(false),
            pending: AtomicBool::new(false),
            calls: AtomicUsize::new(0),
            principal: Mutex::new("session-a".into()),
            last_signal: Mutex::new(None),
        }
    }
}
impl UiBackend for Backend {
    fn authorize(&self, caller: &UiCaller, _: &CapabilityRequest) -> UiResult<UiGrant> {
        if caller.origin() != "app://local" || !self.allowed.load(Ordering::SeqCst) {
            return Err(UiError::Forbidden);
        }
        if !self.authenticated.load(Ordering::SeqCst) {
            return Err(UiError::AuthenticationRequired);
        }
        Ok(UiGrant::new(
            self.principal.lock().clone(),
            identity(),
            ServiceId::new("service-a").unwrap(),
        )
        .unwrap()
        .with_writes_allowed(self.writes.load(Ordering::SeqCst)))
    }
    fn execute<'a>(
        &'a self,
        invocation: UiInvocationV1,
        cancellation: UiCancellation,
    ) -> UiBackendFuture<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        *self.last_signal.lock() = Some(cancellation.clone());
        Box::pin(async move {
            while self.pending.load(Ordering::SeqCst) {
                futures_timer::Delay::new(Duration::from_millis(1)).await;
                if cancellation.is_cancelled() {
                    return Err(UiError::Cancelled);
                }
            }
            match invocation {
                UiInvocationV1::Query(query) => Ok(query.payload),
                UiInvocationV1::Command(_) => Ok(json!({"accepted":true})),
            }
        })
    }
}
fn descriptor(name: &str, mode: CapabilityMode, leader: bool) -> CapabilityDescriptor {
    CapabilityDescriptor {
        name: CapabilityName::new(name).unwrap(),
        version: "1".into(),
        mode,
        visibility: CapabilityVisibility::Local,
        requirements: CapabilityRequirements {
            requires_leader: leader,
            read_only: mode == CapabilityMode::Query,
            idempotency_required: mode == CapabilityMode::Command,
        },
    }
}
fn fixture(limits: UiLimits) -> (UiSurface, UiCaller, Arc<Backend>) {
    let backend = Arc::new(Backend::default());
    let caller = UiCaller::new("surface-a", "app://local").unwrap();
    let catalog = CapabilityCatalog::from_descriptors([
        descriptor("state.inspect", CapabilityMode::Query, false),
        descriptor("state.change", CapabilityMode::Command, false),
        descriptor("state.leader", CapabilityMode::Command, true),
    ])
    .unwrap();
    let surface = UiSurface::new(
        caller.clone(),
        limits,
        catalog,
        [UiTopicBinding {
            topic: "state.changed".into(),
            capability: "state.inspect".into(),
        }],
        backend.clone(),
    )
    .unwrap();
    (surface, caller, backend)
}
fn frame(id: &str, action: UiActionV1) -> UiRequestV1 {
    UiRequestV1 {
        version: 1,
        surface_id: "surface-a".into(),
        request_id: id.into(),
        timeout_ms: 1000,
        action,
    }
}
fn query(id: &str) -> UiRequestV1 {
    frame(
        id,
        UiActionV1::Query {
            query: QueryRequest {
                query_name: "state.inspect".into(),
                query_id: id.into(),
                payload: json!({"content":"private"}),
            },
        },
    )
}
fn block_on<T>(future: impl Future<Output = T>) -> T {
    let mut future = Box::pin(future);
    let mut context = Context::from_waker(Waker::noop());
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}
fn error(reply: UiReplyV1) -> UiError {
    match reply.outcome {
        UiOutcomeV1::Error { code } => code,
        _ => panic!("expected controlled error"),
    }
}
fn value(reply: UiReplyV1) -> Value {
    match reply.outcome {
        UiOutcomeV1::Ok { value } => value,
        _ => panic!("expected success"),
    }
}
fn subscribe(surface: &UiSurface, caller: &UiCaller) -> String {
    let reply = block_on(surface.dispatch(
        caller,
        frame(
            "subscribe",
            UiActionV1::Subscribe {
                capability: "state.inspect".into(),
                topic: "state.changed".into(),
            },
        ),
    ))
    .unwrap();
    value(reply)["subscription_id"]
        .as_str()
        .unwrap()
        .to_string()
}
#[test]
fn authenticated_query_reuses_contract_and_redacts_reply_debug() {
    let (surface, caller, backend) = fixture(UiLimits::default());
    let reply = block_on(surface.dispatch(&caller, query("query-1"))).unwrap();
    assert!(!format!("{reply:?}").contains("private"));
    assert_eq!(value(reply), json!({"content":"private"}));
    assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
    assert_eq!(surface.snapshot().inflight, 0);
}
#[test]
fn caller_origin_capability_and_operational_policy_fail_before_execution() {
    let (surface, caller, backend) = fixture(UiLimits::default());
    let foreign = UiCaller::new("surface-a", "https://other.example").unwrap();
    assert_eq!(
        block_on(surface.dispatch(&foreign, query("query-1"))).unwrap_err(),
        UiError::Forbidden
    );
    backend.authenticated.store(false, Ordering::SeqCst);
    assert_eq!(
        error(block_on(surface.dispatch(&caller, query("query-2"))).unwrap()),
        UiError::AuthenticationRequired
    );
    backend.authenticated.store(true, Ordering::SeqCst);
    for name in ["state.change", "state.leader"] {
        let command = CommandRequest {
            command_name: name.into(),
            command_id: "command-1".into(),
            idempotency_key: Some("operation-1".into()),
            payload: "{}".into(),
        };
        assert_eq!(
            error(
                block_on(
                    surface.dispatch(&caller, frame("command-1", UiActionV1::Command { command }))
                )
                .unwrap()
            ),
            UiError::Forbidden
        );
    }
    backend.writes.store(true, Ordering::SeqCst);
    let command = CommandRequest {
        command_name: "state.leader".into(),
        command_id: "command-2".into(),
        idempotency_key: Some("operation-2".into()),
        payload: "{}".into(),
    };
    assert_eq!(
        error(
            block_on(
                surface.dispatch(&caller, frame("command-2", UiActionV1::Command { command }))
            )
            .unwrap()
        ),
        UiError::Forbidden
    );
    assert_eq!(backend.calls.load(Ordering::SeqCst), 0);
}
#[test]
fn full_event_queue_reports_loss_and_reauthorization_removes_old_session() {
    let (surface, caller, backend) = fixture(UiLimits {
        max_events_per_subscription: 1,
        ..Default::default()
    });
    let id = subscribe(&surface, &caller);
    assert_eq!(
        surface
            .publish("state.changed", json!({"revision":1}))
            .unwrap()
            .delivered,
        1
    );
    assert_eq!(
        surface
            .publish("state.changed", json!({"revision":2}))
            .unwrap()
            .dropped,
        1
    );
    let page = value(
        block_on(surface.dispatch(
            &caller,
            frame(
                "poll-1",
                UiActionV1::Poll {
                    subscription_id: id.clone(),
                },
            ),
        ))
        .unwrap(),
    );
    assert_eq!(page["events"].as_array().unwrap().len(), 1);
    assert_eq!(page["dropped"], 1);
    assert_eq!(surface.snapshot().queued_event_bytes, 0);
    surface
        .publish("state.changed", json!({"revision":3}))
        .unwrap();
    *backend.principal.lock() = "session-b".into();
    assert_eq!(
        error(
            block_on(surface.dispatch(
                &caller,
                frame(
                    "poll-2",
                    UiActionV1::Poll {
                        subscription_id: id
                    }
                )
            ))
            .unwrap()
        ),
        UiError::Forbidden
    );
    assert_eq!(surface.snapshot().subscriptions, 0);
    assert_eq!(surface.snapshot().queued_event_bytes, 0);
}
#[test]
fn subscription_limits_unsubscribe_and_new_generation_do_not_reuse_identity() {
    let (surface, caller, _) = fixture(UiLimits {
        max_subscriptions: 1,
        ..Default::default()
    });
    let first = subscribe(&surface, &caller);
    assert_eq!(
        error(
            block_on(surface.dispatch(
                &caller,
                frame(
                    "subscribe-2",
                    UiActionV1::Subscribe {
                        capability: "state.inspect".into(),
                        topic: "state.changed".into()
                    }
                )
            ))
            .unwrap()
        ),
        UiError::Busy
    );
    value(
        block_on(surface.dispatch(
            &caller,
            frame(
                "unsubscribe",
                UiActionV1::Unsubscribe {
                    subscription_id: first.clone(),
                },
            ),
        ))
        .unwrap(),
    );
    let second = subscribe(&surface, &caller);
    assert_ne!(first, second);
    let (replacement, _, _) = fixture(UiLimits::default());
    assert_ne!(second, subscribe(&replacement, &caller));
}
#[test]
fn one_thread_saturation_cancel_and_dropped_futures_release_admission() {
    let (surface, caller, backend) = fixture(UiLimits {
        max_inflight: 1,
        ..Default::default()
    });
    backend.pending.store(true, Ordering::SeqCst);
    let mut active = Box::pin(surface.dispatch(&caller, query("pending-1")));
    let mut context = Context::from_waker(Waker::noop());
    assert!(matches!(active.as_mut().poll(&mut context), Poll::Pending));
    assert_eq!(
        error(block_on(surface.dispatch(&caller, query("pending-2"))).unwrap()),
        UiError::Busy
    );
    let response = block_on(surface.dispatch(
        &caller,
        frame(
            "cancel",
            UiActionV1::Cancel {
                target_request_id: "pending-1".into(),
            },
        ),
    ))
    .unwrap();
    assert_eq!(value(response), true);
    assert_eq!(error(block_on(active).unwrap()), UiError::Cancelled);
    assert_eq!(surface.snapshot().inflight, 0);
    let mut abandoned = Box::pin(surface.dispatch(&caller, query("abandoned")));
    assert!(matches!(
        abandoned.as_mut().poll(&mut context),
        Poll::Pending
    ));
    drop(abandoned);
    assert_eq!(surface.snapshot().inflight, 0);
    assert!(backend.last_signal.lock().as_ref().unwrap().is_cancelled());
}
#[test]
fn deadline_offline_reconnect_and_close_are_explicit_and_bounded() {
    let (surface, caller, backend) = fixture(UiLimits::default());
    backend.pending.store(true, Ordering::SeqCst);
    let mut request = query("deadline");
    request.timeout_ms = 1;
    assert_eq!(
        error(block_on(surface.dispatch(&caller, request)).unwrap()),
        UiError::DeadlineExceeded
    );
    assert_eq!(surface.snapshot().inflight, 0);
    let id = subscribe(&surface, &caller);
    surface.publish("state.changed", json!(1)).unwrap();
    surface
        .set_connection_state(UiConnectionState::Offline)
        .unwrap();
    assert_eq!(surface.snapshot().subscriptions, 0);
    assert_eq!(surface.snapshot().queued_event_bytes, 0);
    assert_eq!(
        error(block_on(surface.dispatch(&caller, query("offline"))).unwrap()),
        UiError::Unavailable
    );
    surface
        .set_connection_state(UiConnectionState::Connected)
        .unwrap();
    backend.pending.store(false, Ordering::SeqCst);
    assert_eq!(
        error(
            block_on(surface.dispatch(
                &caller,
                frame(
                    "stale",
                    UiActionV1::Poll {
                        subscription_id: id
                    }
                )
            ))
            .unwrap()
        ),
        UiError::SubscriptionNotFound
    );
    assert!(matches!(
        block_on(surface.dispatch(&caller, query("reconnected")))
            .unwrap()
            .outcome,
        UiOutcomeV1::Ok { .. }
    ));
    surface.close();
    assert_eq!(
        error(block_on(surface.dispatch(&caller, query("closed"))).unwrap()),
        UiError::Closed
    );
    assert_eq!(
        surface.set_connection_state(UiConnectionState::Connected),
        Err(UiError::Closed)
    );
}
#[test]
fn native_close_cancels_pending_work_without_claiming_rollback() {
    let (surface, caller, backend) = fixture(UiLimits::default());
    backend.pending.store(true, Ordering::SeqCst);
    let mut pending = Box::pin(surface.dispatch(&caller, query("pending")));
    assert!(matches!(
        pending
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop())),
        Poll::Pending
    ));
    surface.close();
    assert_eq!(error(block_on(pending).unwrap()), UiError::Cancelled);
    assert_eq!(surface.snapshot().inflight, 0);
}

#[test]
fn degraded_surface_reports_state_and_keeps_write_policy_with_the_owner() {
    let (surface, caller, backend) = fixture(UiLimits::default());
    surface
        .set_connection_state(UiConnectionState::Degraded)
        .unwrap();
    let reply = block_on(surface.dispatch(&caller, query("degraded-read"))).unwrap();
    assert_eq!(reply.state, UiConnectionState::Degraded);
    assert_eq!(value(reply), json!({"content":"private"}));
    let command = CommandRequest {
        command_name: "state.change".into(),
        command_id: "degraded-write".into(),
        idempotency_key: Some("degraded-operation".into()),
        payload: "{}".into(),
    };
    let reply = block_on(surface.dispatch(
        &caller,
        frame("degraded-write", UiActionV1::Command { command }),
    ))
    .unwrap();
    assert_eq!(reply.state, UiConnectionState::Degraded);
    assert_eq!(error(reply), UiError::Forbidden);
    assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
    assert_eq!(surface.snapshot().inflight, 0);
}
