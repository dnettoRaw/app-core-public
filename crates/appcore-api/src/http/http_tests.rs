// =============================================================================
//        #######
//     ###       ###     F: http_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:42:05 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 13:24:05 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::{
    command_handler, diagnostics_handler, health_handler, private_status_handler,
    public_status_handler, query_handler, update_required_handler, CommandRequest, CommandResponse,
    HttpApiConfig, HttpState, RuntimeStaticInfo,
};
use crate::api::{ApiRequest, ApiResponse};
use crate::command_contract::CommandRequestValidationError;
use crate::query_contract::{QueryRequest, QueryResponse};
use crate::{ApiRouter, QueryEndpoint, QueryName};
use appcore_contracts::{ApplicationId, ApplicationManifestV1, RuntimeRequirements, ServiceId};
use appcore_core::{
    AppId, AppPlugin, AuditCategory, AuditOutcome, CommandEnvelope, CommandName, CommandRegistry,
    CommandResult, DecisionRegistry, EventEnvelope, EventName, EventRegistry, NodeId,
    RuntimeBuilder, RuntimeContext, RuntimeContractVersion, RuntimeController, RuntimeIdentity,
    RuntimeResult, StateRegistry, SyncGroup,
};
use appcore_security::CommandTokenError;
use axum::body::{to_bytes, Body};
use axum::http::{HeaderValue, Request};
use axum::response::IntoResponse;
use axum::Json;
use parking_lot::Mutex;
use std::sync::Arc;
use tower::ServiceExt;

#[test]
fn http_api_config_default() {
    let cfg = HttpApiConfig::default();
    assert_eq!(cfg.host, "127.0.0.1");
    assert_eq!(cfg.port, 8080);
    assert!(!cfg.enabled);
    assert_eq!(cfg.max_payload_bytes, 65_536);
}

#[test]
fn http_auth_default_is_fail_closed() {
    let auth = super::HttpCommandAuth::default();
    assert!(auth.require_token);
    assert!(!auth.public_status);
    assert!(auth.verifier.is_none());
}

#[test]
fn default_host_rejects_private_routes_and_keeps_health_public() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let controller = Arc::new(Mutex::new(build_controller().unwrap()));
        let host = super::RuntimeHttpHost::with_controller(
            HttpApiConfig::default(),
            test_snapshot(),
            controller,
        );
        let command = Request::post("/v1/command")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"command_name":"runtime.ping","command_id":"command-1","payload":""}"#,
            ))
            .unwrap();
        let query = Request::post("/v1/query")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"query_name":"runtime.status","query_id":"query-1","payload":{}}"#,
            ))
            .unwrap();
        let health = Request::get("/v1/health").body(Body::empty()).unwrap();

        assert_eq!(
            host.router().oneshot(command).await.unwrap().status(),
            axum::http::StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            host.router().oneshot(query).await.unwrap().status(),
            axum::http::StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            host.router().oneshot(health).await.unwrap().status(),
            axum::http::StatusCode::OK
        );
    });
}

#[test]
fn removed_routes_return_the_exact_update_wall() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let (status, body) = runtime.block_on(update_required_handler());
    assert_eq!(status, axum::http::StatusCode::UPGRADE_REQUIRED);
    assert_eq!(body, "NO MORE SUPPORTED PLEASE UPDATE");
}

#[test]
fn router_rejects_body_above_configured_limit_before_deserialization() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let host = super::RuntimeHttpHost::new(
            HttpApiConfig {
                max_payload_bytes: 64,
                ..HttpApiConfig::default()
            },
            test_snapshot(),
        );
        let request = Request::post("/v1/command")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"command_name":"runtime.ping","command_id":"command-1","payload":"too-large"}"#,
            ))
            .unwrap();

        let response = host.router().oneshot(request).await.unwrap();

        assert_eq!(response.status(), axum::http::StatusCode::PAYLOAD_TOO_LARGE);
    });
}

#[test]
fn command_request_parse() {
    let json = r#"{"command_name":"runtime.ping","command_id":"cmd-1","idempotency_key":null,"payload":"hello"}"#;
    let parsed = serde_json::from_str::<CommandRequest>(json);
    assert!(parsed.is_ok());
}

#[test]
fn command_response_serialize() {
    let response = CommandResponse {
        accepted: true,
        message: None,
        events: vec![super::CommandResponseEvent {
            event_name: "runtime.pong".to_string(),
            event_id: "evt-cmd-1".to_string(),
        }],
    };
    let json = serde_json::to_string(&response);
    assert!(json.is_ok());
}

struct PingHandler;
struct AppEchoQuery;

impl appcore_core::CommandHandler for PingHandler {
    fn command_name(&self) -> CommandName {
        CommandName::new("runtime.ping".to_string()).unwrap()
    }
    fn handle(
        &self,
        command: &CommandEnvelope,
        _context: &dyn RuntimeContext,
    ) -> RuntimeResult<CommandResult> {
        let event = EventEnvelope::new(
            EventName::new("runtime.pong".to_string()).unwrap(),
            format!("evt-{}", command.command_id),
            command.app_id.clone(),
            command.node_id.clone(),
            0,
            vec![],
        )?;
        Ok(CommandResult::accepted(vec![event]))
    }
}

impl QueryEndpoint for AppEchoQuery {
    fn query_name(&self) -> &QueryName {
        static NAME: std::sync::OnceLock<QueryName> = std::sync::OnceLock::new();
        NAME.get_or_init(|| QueryName::new("note.get".to_string()).unwrap())
    }

    fn handle_query(&self, request: ApiRequest) -> RuntimeResult<ApiResponse> {
        Ok(ApiResponse {
            status_code: 200,
            payload: request.payload,
        })
    }
}

struct TestPlugin;
struct AllowVerifier;
struct DenyVerifier;
struct ForbidVerifier;
struct RejectCommandPolicy(super::CommandCapabilityPolicyError);
struct RejectQueryPolicy;

impl super::CommandTokenVerifier for AllowVerifier {
    fn verify_command_token(
        &self,
        _token: &str,
        _command_name: &str,
    ) -> Result<(), CommandTokenError> {
        Ok(())
    }
    fn verify_query_token(&self, _token: &str, _query_name: &str) -> Result<(), CommandTokenError> {
        Ok(())
    }
}

impl super::CommandTokenVerifier for DenyVerifier {
    fn verify_command_token(
        &self,
        _token: &str,
        _command_name: &str,
    ) -> Result<(), CommandTokenError> {
        Err(CommandTokenError::Unauthorized)
    }
    fn verify_query_token(&self, _token: &str, _query_name: &str) -> Result<(), CommandTokenError> {
        Err(CommandTokenError::Unauthorized)
    }
}

impl super::CommandTokenVerifier for ForbidVerifier {
    fn verify_command_token(
        &self,
        _token: &str,
        _command_name: &str,
    ) -> Result<(), CommandTokenError> {
        Err(CommandTokenError::Forbidden)
    }
    fn verify_query_token(&self, _token: &str, _query_name: &str) -> Result<(), CommandTokenError> {
        Err(CommandTokenError::Forbidden)
    }
}

impl super::CommandCapabilityPolicy for RejectCommandPolicy {
    fn authorize_command(
        &self,
        _command_name: &str,
        _idempotency_key: Option<&str>,
        _now_ms: u64,
    ) -> Result<(), super::CommandCapabilityPolicyError> {
        Err(self.0.clone())
    }
}

impl super::CommandCapabilityPolicy for RejectQueryPolicy {
    fn authorize_command(
        &self,
        _command_name: &str,
        _idempotency_key: Option<&str>,
        _now_ms: u64,
    ) -> Result<(), super::CommandCapabilityPolicyError> {
        Ok(())
    }

    fn authorize_query(
        &self,
        _query_name: &str,
        _now_ms: u64,
    ) -> Result<(), super::CommandCapabilityPolicyError> {
        Err(super::CommandCapabilityPolicyError::CapabilityNotDeclared)
    }
}

impl AppPlugin for TestPlugin {
    fn application_manifest(&self) -> ApplicationManifestV1 {
        ApplicationManifestV1::new(
            ApplicationId::new("minimal-app").unwrap(),
            "1.0.0",
            "Minimal App",
            "AppCore Test",
            ServiceId::new("minimal-app").unwrap(),
            RuntimeRequirements::new("1.0.0-rc.3", "1").unwrap(),
        )
        .unwrap()
    }
    fn identity(&self, node_id: NodeId) -> RuntimeIdentity {
        RuntimeIdentity {
            app_id: AppId::new("minimal-app".to_string()).unwrap(),
            app_family: appcore_core::AppFamily::new("example-family".to_string()).unwrap(),
            sync_group: SyncGroup::new("dev".to_string()).unwrap(),
            runtime_contract: RuntimeContractVersion::new(1),
            node_id,
        }
    }
    fn register_commands(&self, registry: &mut CommandRegistry) -> RuntimeResult<()> {
        registry.register(CommandName::new("runtime.ping".to_string()).unwrap())
    }
    fn register_events(&self, registry: &mut EventRegistry) -> RuntimeResult<()> {
        registry.register(EventName::new("runtime.pong".to_string()).unwrap())
    }
    fn register_states(&self, _registry: &mut StateRegistry) -> RuntimeResult<()> {
        Ok(())
    }
    fn register_decisions(&self, _registry: &mut DecisionRegistry) -> RuntimeResult<()> {
        Ok(())
    }
    fn register_handlers(&self, bus: &mut appcore_core::CommandBus) -> RuntimeResult<()> {
        bus.register_handler(PingHandler)
    }
}

fn build_controller() -> Option<RuntimeController> {
    let plugin = TestPlugin;
    let mut builder = RuntimeBuilder::new();
    if builder
        .with_plugin(&plugin, NodeId::new("node-a".to_string()).unwrap())
        .is_err()
    {
        return None;
    }
    let instance = match builder.build() {
        Ok(instance) => instance,
        Err(_) => return None,
    };
    let mut controller = RuntimeController::new(instance);
    for event in [
        appcore_core::RuntimeLifecycleEvent::ConfigLoaded,
        appcore_core::RuntimeLifecycleEvent::SecurityChecked,
        appcore_core::RuntimeLifecycleEvent::StorageOpened,
        appcore_core::RuntimeLifecycleEvent::ApiStarted,
    ] {
        if controller.apply_lifecycle_event(event).is_err() {
            return None;
        }
    }
    Some(controller)
}

fn test_snapshot() -> RuntimeStaticInfo {
    RuntimeStaticInfo {
        app_id: "minimal-app".to_string(),
        node_id: "node-a".to_string(),
        tenant_id: "minimal-app".to_string(),
        cluster_id: "dev".to_string(),
        core_id: "node-a".to_string(),
        operation_mode: "read_write".to_string(),
        storage_status: "Online".to_string(),
        security_ok: true,
        api_enabled: true,
        sync_enabled: true,
        sync_role: "leader".to_string(),
        sync_log_len: 1,
        sync_log_path: Some("/tmp/sync.log".to_string()),
        sync_checkpoint_path: Some("/tmp/sync.checkpoints".to_string()),
        sync_peers: vec!["http://127.0.0.1:39201".to_string()],
        sync_dns_enabled: true,
        sync_dns_seeds: vec!["localhost".to_string()],
        sync_dns_default_port: 39201,
        idempotency_ttl_ms: 86_400_000,
        idempotency_path: Some("/tmp/idempotency.txt".to_string()),
    }
}

fn status_state(public_status: bool) -> HttpState {
    let supervisor = appcore_supervisor::Supervisor::new();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    supervisor.reconcile(now).unwrap();
    HttpState {
        static_info: test_snapshot(),
        sync_log: None,
        tick_counter: None,
        operation_mode: None,
        supervisor: Some(supervisor),
        command_policy: None,
        controller: None,
        app_query_router: None,
        auth: super::HttpCommandAuth {
            require_token: false,
            public_status,
            verifier: Some(Arc::new(AllowVerifier)),
        },
        max_payload_bytes: 65_536,
        clock: Arc::new(appcore_core::SystemClock::new()),
    }
}

#[derive(Debug)]
struct FixedClock(u64);

impl appcore_core::Clock for FixedClock {
    fn now_ms(&self) -> u64 {
        self.0
    }
}

fn bearer_headers() -> axum::http::HeaderMap {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        "authorization",
        HeaderValue::from_static("Bearer test-token"),
    );
    headers
}

#[test]
fn public_status_is_reduced_and_policy_controlled() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let disabled = public_status_handler(axum::extract::State(status_state(false))).await;
        assert_eq!(disabled.status(), axum::http::StatusCode::UNAUTHORIZED);

        let response = public_status_handler(axum::extract::State(status_state(true))).await;
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "online");
        assert!(json.get("app_id").is_none());
        assert!(json.get("storage_status").is_none());
    });
}

#[test]
fn private_status_and_diagnostics_always_require_scoped_tokens() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let private = private_status_handler(
            axum::extract::State(status_state(true)),
            axum::http::HeaderMap::new(),
        )
        .await;
        assert_eq!(private.status(), axum::http::StatusCode::UNAUTHORIZED);

        let private =
            private_status_handler(axum::extract::State(status_state(true)), bearer_headers())
                .await;
        assert_eq!(private.status(), axum::http::StatusCode::OK);
        let body = to_bytes(private.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["app_id"], "minimal-app");

        let diagnostics =
            diagnostics_handler(axum::extract::State(status_state(true)), bearer_headers()).await;
        assert_eq!(diagnostics.status(), axum::http::StatusCode::OK);
        let body = to_bytes(diagnostics.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("supervisor").is_some());
        assert!(json.get("sync").is_some());
    });
}

#[test]
fn health_is_derived_from_runtime_state() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let healthy = health_handler(axum::extract::State(status_state(true))).await;
        assert_eq!(healthy.status(), axum::http::StatusCode::OK);

        let mut unhealthy_state = status_state(true);
        unhealthy_state.static_info.security_ok = false;
        let unhealthy = health_handler(axum::extract::State(unhealthy_state)).await;
        assert_eq!(
            unhealthy.status(),
            axum::http::StatusCode::SERVICE_UNAVAILABLE
        );
    });
}

#[test]
fn live_http_with_stopped_reconcile_sequence_is_unhealthy() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let supervisor = appcore_supervisor::Supervisor::with_watchdog_config(
            appcore_supervisor::WatchdogConfig {
                enabled: true,
                check_interval_ms: 5,
                stall_timeout_ms: 10,
            },
        )
        .unwrap();
        supervisor.reconcile(100).unwrap();
        let mut state = status_state(true);
        state.supervisor = Some(supervisor);
        state.clock = Arc::new(FixedClock(111));

        let response = health_handler(axum::extract::State(state)).await;

        assert_eq!(
            response.status(),
            axum::http::StatusCode::SERVICE_UNAVAILABLE
        );
    });
}

#[test]
fn command_handler_runtime_ping_returns_accepted() {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => return,
    };
    runtime.block_on(async {
        let controller = match build_controller() {
            Some(controller) => Arc::new(Mutex::new(controller)),
            None => return,
        };
        let request = CommandRequest {
            command_name: "runtime.ping".to_string(),
            command_id: "cmd-1".to_string(),
            idempotency_key: None,
            payload: "hello".to_string(),
        };
        let state = HttpState {
            static_info: test_snapshot(),
            sync_log: None,
            tick_counter: None,
            operation_mode: None,
            supervisor: None,
            command_policy: None,
            controller: Some(Arc::clone(&controller)),
            app_query_router: None,
            auth: super::HttpCommandAuth::insecure_local_for_testing(),
            max_payload_bytes: 65_536,
            clock: Arc::new(appcore_core::SystemClock::new()),
        };
        let response = command_handler(
            axum::extract::State(state),
            axum::http::HeaderMap::new(),
            Json(request),
        )
        .await
        .into_response();
        assert_eq!(response.status().as_u16(), 200);
    });
}

#[test]
fn command_trace_headers_propagate_to_command_event_and_audit() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let controller = Arc::new(Mutex::new(build_controller().unwrap()));
        let state = HttpState {
            static_info: test_snapshot(),
            sync_log: None,
            tick_counter: None,
            operation_mode: None,
            supervisor: None,
            command_policy: None,
            controller: Some(Arc::clone(&controller)),
            app_query_router: None,
            auth: super::HttpCommandAuth::insecure_local_for_testing(),
            max_payload_bytes: 65_536,
            clock: Arc::new(appcore_core::SystemClock::new()),
        };
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            "x-appcore-trace-id",
            HeaderValue::from_static("trace-http-1"),
        );
        headers.insert("x-appcore-span-id", HeaderValue::from_static("span-http-1"));
        let request = CommandRequest {
            command_name: "runtime.ping".to_string(),
            command_id: "cmd-traced".to_string(),
            idempotency_key: None,
            payload: String::new(),
        };

        let response = command_handler(axum::extract::State(state), headers, Json(request)).await;
        assert_eq!(
            response.into_response().status(),
            axum::http::StatusCode::OK
        );
        let guard = controller.lock();
        let entries = guard.instance().audit_log().entries();
        assert!(entries.iter().any(|entry| {
            entry.category == AuditCategory::Command
                && entry.trace.as_ref().map(|trace| trace.trace_id.as_str()) == Some("trace-http-1")
        }));
        assert!(entries.iter().any(|entry| {
            entry.category == AuditCategory::Event
                && entry.trace.as_ref().map(|trace| trace.trace_id.as_str()) == Some("trace-http-1")
        }));
    });
}

#[test]
fn command_handler_unknown_returns_controlled_error() {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => return,
    };
    runtime.block_on(async {
        let controller = match build_controller() {
            Some(controller) => Arc::new(Mutex::new(controller)),
            None => return,
        };
        let request = CommandRequest {
            command_name: "runtime.unknown".to_string(),
            command_id: "cmd-1".to_string(),
            idempotency_key: None,
            payload: "hello".to_string(),
        };
        let state = HttpState {
            static_info: test_snapshot(),
            sync_log: None,
            tick_counter: None,
            operation_mode: None,
            supervisor: None,
            command_policy: None,
            controller: Some(controller),
            app_query_router: None,
            auth: super::HttpCommandAuth::insecure_local_for_testing(),
            max_payload_bytes: 65_536,
            clock: Arc::new(appcore_core::SystemClock::new()),
        };
        let response = command_handler(
            axum::extract::State(state),
            axum::http::HeaderMap::new(),
            Json(request),
        )
        .await
        .into_response();
        assert_eq!(response.status().as_u16(), 400);
    });
}

#[test]
fn command_handler_policy_rejection_returns_controlled_error() {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => return,
    };
    runtime.block_on(async {
        let controller = match build_controller() {
            Some(controller) => Arc::new(Mutex::new(controller)),
            None => return,
        };
        let request = CommandRequest {
            command_name: "runtime.ping".to_string(),
            command_id: "cmd-1".to_string(),
            idempotency_key: None,
            payload: "hello".to_string(),
        };
        let state = HttpState {
            static_info: test_snapshot(),
            sync_log: None,
            tick_counter: None,
            operation_mode: None,
            supervisor: None,
            command_policy: Some(Arc::new(RejectCommandPolicy(
                super::CommandCapabilityPolicyError::RequiresLeader,
            ))),
            controller: Some(controller),
            app_query_router: None,
            auth: super::HttpCommandAuth::insecure_local_for_testing(),
            max_payload_bytes: 65_536,
            clock: Arc::new(appcore_core::SystemClock::new()),
        };
        let response = command_handler(
            axum::extract::State(state),
            axum::http::HeaderMap::new(),
            Json(request),
        )
        .await
        .into_response();
        assert_eq!(response.status().as_u16(), 409);
    });
}

#[test]
fn command_handler_same_idempotency_key_does_not_duplicate_event() {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => return,
    };
    runtime.block_on(async {
        let controller = match build_controller() {
            Some(controller) => Arc::new(Mutex::new(controller)),
            None => return,
        };
        let state = HttpState {
            static_info: test_snapshot(),
            sync_log: None,
            tick_counter: None,
            operation_mode: None,
            supervisor: None,
            command_policy: None,
            controller: Some(controller.clone()),
            app_query_router: None,
            auth: super::HttpCommandAuth::insecure_local_for_testing(),
            max_payload_bytes: 65_536,
            clock: Arc::new(appcore_core::SystemClock::new()),
        };
        let first = CommandRequest {
            command_name: "runtime.ping".to_string(),
            command_id: "cmd-1".to_string(),
            idempotency_key: Some("dup-1".to_string()),
            payload: "hello".to_string(),
        };
        let second = CommandRequest {
            command_name: "runtime.ping".to_string(),
            command_id: "cmd-2".to_string(),
            idempotency_key: Some("dup-1".to_string()),
            payload: "hello".to_string(),
        };
        let _ = command_handler(
            axum::extract::State(state.clone()),
            axum::http::HeaderMap::new(),
            Json(first),
        )
        .await
        .into_response();
        let second_response = command_handler(
            axum::extract::State(state),
            axum::http::HeaderMap::new(),
            Json(second),
        )
        .await
        .into_response();
        let body = to_bytes(second_response.into_body(), usize::MAX).await;
        assert!(body.is_ok());
        let body = match body {
            Ok(body) => body,
            Err(_) => return,
        };
        let parsed = serde_json::from_slice::<CommandResponse>(&body);
        assert!(parsed.is_ok());
        let parsed = match parsed {
            Ok(value) => value,
            Err(_) => return,
        };
        assert!(parsed.accepted);
        let event_count = controller.lock().instance().event_bus().len();
        assert_eq!(event_count, 1);
    });
}

#[test]
fn command_handler_without_token_returns_401() {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => return,
    };
    runtime.block_on(async {
        let controller = match build_controller() {
            Some(controller) => Arc::new(Mutex::new(controller)),
            None => return,
        };
        let state = HttpState {
            static_info: test_snapshot(),
            sync_log: None,
            tick_counter: None,
            operation_mode: None,
            supervisor: None,
            command_policy: None,
            controller: Some(controller),
            app_query_router: None,
            auth: super::HttpCommandAuth {
                require_token: true,
                public_status: false,
                verifier: Some(Arc::new(AllowVerifier)),
            },
            max_payload_bytes: 65_536,
            clock: Arc::new(appcore_core::SystemClock::new()),
        };
        let request = CommandRequest {
            command_name: "runtime.ping".to_string(),
            command_id: "cmd-1".to_string(),
            idempotency_key: None,
            payload: "hello".to_string(),
        };
        let response = command_handler(
            axum::extract::State(state),
            axum::http::HeaderMap::new(),
            Json(request),
        )
        .await
        .into_response();
        assert_eq!(response.status().as_u16(), 401);
    });
}

#[test]
fn command_handler_invalid_token_returns_401() {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => return,
    };
    runtime.block_on(async {
        let controller = match build_controller() {
            Some(controller) => Arc::new(Mutex::new(controller)),
            None => return,
        };
        let state = HttpState {
            static_info: test_snapshot(),
            sync_log: None,
            tick_counter: None,
            operation_mode: None,
            supervisor: None,
            command_policy: None,
            controller: Some(Arc::clone(&controller)),
            app_query_router: None,
            auth: super::HttpCommandAuth {
                require_token: true,
                public_status: false,
                verifier: Some(Arc::new(DenyVerifier)),
            },
            max_payload_bytes: 65_536,
            clock: Arc::new(appcore_core::SystemClock::new()),
        };
        let request = CommandRequest {
            command_name: "runtime.ping".to_string(),
            command_id: "cmd-1".to_string(),
            idempotency_key: None,
            payload: "sensitive-payload-marker".to_string(),
        };
        let mut headers = axum::http::HeaderMap::new();
        let inserted = headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer sensitive-token-marker"),
        );
        assert!(inserted.is_none());
        let response = command_handler(axum::extract::State(state), headers, Json(request))
            .await
            .into_response();
        assert_eq!(response.status().as_u16(), 401);
        let entries = controller.lock().instance().audit_log().entries();
        let rejection = entries
            .iter()
            .find(|entry| {
                entry.category == AuditCategory::Command
                    && entry.operation_id == "cmd-1"
                    && entry.outcome == AuditOutcome::Rejected
            })
            .unwrap();
        assert_eq!(rejection.operation_name, "runtime.ping");
        assert_eq!(
            rejection.message.as_deref(),
            Some("command authorization rejected")
        );
        let serialized = serde_json::to_string(rejection).unwrap();
        assert!(!serialized.contains("sensitive-token-marker"));
        assert!(!serialized.contains("sensitive-payload-marker"));
    });
}

#[test]
fn command_handler_forbidden_token_returns_403() {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => return,
    };
    runtime.block_on(async {
        let controller = match build_controller() {
            Some(controller) => Arc::new(Mutex::new(controller)),
            None => return,
        };
        let state = HttpState {
            static_info: test_snapshot(),
            sync_log: None,
            tick_counter: None,
            operation_mode: None,
            supervisor: None,
            command_policy: None,
            controller: Some(controller),
            app_query_router: None,
            auth: super::HttpCommandAuth {
                require_token: true,
                public_status: false,
                verifier: Some(Arc::new(ForbidVerifier)),
            },
            max_payload_bytes: 65_536,
            clock: Arc::new(appcore_core::SystemClock::new()),
        };
        let request = CommandRequest {
            command_name: "runtime.ping".to_string(),
            command_id: "cmd-1".to_string(),
            idempotency_key: None,
            payload: "hello".to_string(),
        };
        let mut headers = axum::http::HeaderMap::new();
        let inserted = headers.insert("authorization", HeaderValue::from_static("Bearer blocked"));
        assert!(inserted.is_none());
        let response = command_handler(axum::extract::State(state), headers, Json(request))
            .await
            .into_response();
        assert_eq!(response.status().as_u16(), 403);
    });
}

#[test]
fn command_handler_valid_token_accepts_ping() {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => return,
    };
    runtime.block_on(async {
        let controller = match build_controller() {
            Some(controller) => Arc::new(Mutex::new(controller)),
            None => return,
        };
        let state = HttpState {
            static_info: test_snapshot(),
            sync_log: None,
            tick_counter: None,
            operation_mode: None,
            supervisor: None,
            command_policy: None,
            controller: Some(controller),
            app_query_router: None,
            auth: super::HttpCommandAuth {
                require_token: true,
                public_status: false,
                verifier: Some(Arc::new(AllowVerifier)),
            },
            max_payload_bytes: 65_536,
            clock: Arc::new(appcore_core::SystemClock::new()),
        };
        let request = CommandRequest {
            command_name: "runtime.ping".to_string(),
            command_id: "cmd-1".to_string(),
            idempotency_key: None,
            payload: "hello".to_string(),
        };
        let mut headers = axum::http::HeaderMap::new();
        let inserted = headers.insert("authorization", HeaderValue::from_static("Bearer ok"));
        assert!(inserted.is_none());
        let response = command_handler(axum::extract::State(state), headers, Json(request))
            .await
            .into_response();
        assert_eq!(response.status().as_u16(), 200);
    });
}

#[test]
fn command_handler_requires_explicit_insecure_test_mode() {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => return,
    };
    runtime.block_on(async {
        let controller = match build_controller() {
            Some(controller) => Arc::new(Mutex::new(controller)),
            None => return,
        };
        let state = HttpState {
            static_info: test_snapshot(),
            sync_log: None,
            tick_counter: None,
            operation_mode: None,
            supervisor: None,
            command_policy: None,
            controller: Some(controller),
            app_query_router: None,
            auth: super::HttpCommandAuth::insecure_local_for_testing(),
            max_payload_bytes: 65_536,
            clock: Arc::new(appcore_core::SystemClock::new()),
        };
        let request = CommandRequest {
            command_name: "runtime.ping".to_string(),
            command_id: "cmd-1".to_string(),
            idempotency_key: None,
            payload: "hello".to_string(),
        };
        let response = command_handler(
            axum::extract::State(state),
            axum::http::HeaderMap::new(),
            Json(request),
        )
        .await
        .into_response();
        assert_eq!(response.status().as_u16(), 200);
    });
}

#[test]
fn command_request_validation_rules() {
    let valid = CommandRequest {
        command_name: "runtime.ping".to_string(),
        command_id: "cmd-1".to_string(),
        idempotency_key: Some("idemp-1".to_string()),
        payload: "ok".to_string(),
    };
    assert!(valid.validate(65_536).is_ok());

    let empty_name = CommandRequest {
        command_name: "".to_string(),
        ..valid.clone()
    };
    assert_eq!(
        empty_name.validate(65_536),
        Err(CommandRequestValidationError::EmptyCommandName)
    );

    let empty_id = CommandRequest {
        command_id: "".to_string(),
        ..valid.clone()
    };
    assert_eq!(
        empty_id.validate(65_536),
        Err(CommandRequestValidationError::EmptyCommandId)
    );

    let invalid_name = CommandRequest {
        command_name: "runtime ping".to_string(),
        ..valid.clone()
    };
    assert_eq!(
        invalid_name.validate(65_536),
        Err(CommandRequestValidationError::InvalidCommandName)
    );

    let invalid_key = CommandRequest {
        idempotency_key: Some("".to_string()),
        ..valid.clone()
    };
    assert_eq!(
        invalid_key.validate(65_536),
        Err(CommandRequestValidationError::InvalidIdempotencyKey)
    );

    let too_large = CommandRequest {
        payload: "x".repeat(9),
        ..valid
    };
    assert_eq!(
        too_large.validate(8),
        Err(CommandRequestValidationError::PayloadTooLarge)
    );
}

#[test]
fn command_handler_payload_too_large_returns_413() {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => return,
    };
    runtime.block_on(async {
        let controller = match build_controller() {
            Some(controller) => Arc::new(Mutex::new(controller)),
            None => return,
        };
        let state = HttpState {
            static_info: test_snapshot(),
            sync_log: None,
            tick_counter: None,
            operation_mode: None,
            supervisor: None,
            command_policy: None,
            controller: Some(controller),
            app_query_router: None,
            auth: super::HttpCommandAuth::insecure_local_for_testing(),
            max_payload_bytes: 4,
            clock: Arc::new(appcore_core::SystemClock::new()),
        };
        let request = CommandRequest {
            command_name: "runtime.ping".to_string(),
            command_id: "cmd-1".to_string(),
            idempotency_key: None,
            payload: "hello".to_string(),
        };
        let response = command_handler(
            axum::extract::State(state),
            axum::http::HeaderMap::new(),
            Json(request),
        )
        .await
        .into_response();
        assert_eq!(response.status().as_u16(), 413);
    });
}

#[test]
fn command_handler_mutating_command_without_idempotency_returns_400() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    assert!(runtime.is_ok());
    let runtime = match runtime {
        Ok(runtime) => runtime,
        Err(_) => return,
    };
    runtime.block_on(async {
        let controller = match build_controller() {
            Some(controller) => Arc::new(Mutex::new(controller)),
            None => return,
        };
        let state = HttpState {
            static_info: test_snapshot(),
            sync_log: None,
            tick_counter: None,
            operation_mode: None,
            supervisor: None,
            command_policy: None,
            controller: Some(controller),
            app_query_router: None,
            auth: super::HttpCommandAuth::insecure_local_for_testing(),
            max_payload_bytes: 65_536,
            clock: Arc::new(appcore_core::SystemClock::new()),
        };
        let request = CommandRequest {
            command_name: "runtime.test.write".to_string(),
            command_id: "cmd-1".to_string(),
            idempotency_key: None,
            payload: "hello".to_string(),
        };
        let response = command_handler(
            axum::extract::State(state),
            axum::http::HeaderMap::new(),
            Json(request),
        )
        .await
        .into_response();
        assert_eq!(response.status().as_u16(), 400);
    });
}

#[test]
fn query_handler_without_token_returns_401() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    assert!(runtime.is_ok());
    let runtime = match runtime {
        Ok(v) => v,
        Err(_) => return,
    };
    runtime.block_on(async {
        let state = HttpState {
            static_info: test_snapshot(),
            sync_log: None,
            tick_counter: None,
            operation_mode: None,
            supervisor: None,
            command_policy: None,
            controller: None,
            app_query_router: None,
            auth: super::HttpCommandAuth {
                require_token: true,
                public_status: false,
                verifier: Some(Arc::new(AllowVerifier)),
            },
            max_payload_bytes: 65_536,
            clock: Arc::new(appcore_core::SystemClock::new()),
        };
        let request = QueryRequest {
            query_name: "runtime.status".to_string(),
            query_id: "qry-1".to_string(),
            payload: serde_json::json!({}),
        };
        let response = query_handler(
            axum::extract::State(state),
            axum::http::HeaderMap::new(),
            Json(request),
        )
        .await
        .into_response();
        assert_eq!(response.status().as_u16(), 401);
    });
}

#[test]
fn query_handler_invalid_query_name_returns_400() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    assert!(runtime.is_ok());
    let runtime = match runtime {
        Ok(v) => v,
        Err(_) => return,
    };
    runtime.block_on(async {
        let mut headers = axum::http::HeaderMap::new();
        let _ = headers.insert("authorization", HeaderValue::from_static("Bearer ok"));
        let state = HttpState {
            static_info: test_snapshot(),
            sync_log: None,
            tick_counter: None,
            operation_mode: None,
            supervisor: None,
            command_policy: None,
            controller: None,
            app_query_router: None,
            auth: super::HttpCommandAuth {
                require_token: true,
                public_status: false,
                verifier: Some(Arc::new(AllowVerifier)),
            },
            max_payload_bytes: 65_536,
            clock: Arc::new(appcore_core::SystemClock::new()),
        };
        let request = QueryRequest {
            query_name: "runtime status".to_string(),
            query_id: "qry-1".to_string(),
            payload: serde_json::json!({}),
        };
        let response = query_handler(axum::extract::State(state), headers, Json(request))
            .await
            .into_response();
        assert_eq!(response.status().as_u16(), 400);
    });
}

#[test]
fn query_handler_payload_too_large_returns_413() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    assert!(runtime.is_ok());
    let runtime = match runtime {
        Ok(runtime) => runtime,
        Err(_) => return,
    };
    runtime.block_on(async {
        let state = HttpState {
            static_info: test_snapshot(),
            sync_log: None,
            tick_counter: None,
            operation_mode: None,
            supervisor: None,
            command_policy: None,
            controller: None,
            app_query_router: None,
            auth: super::HttpCommandAuth::insecure_local_for_testing(),
            max_payload_bytes: 4,
            clock: Arc::new(appcore_core::SystemClock::new()),
        };
        let request = QueryRequest {
            query_name: "runtime.status".to_string(),
            query_id: "qry-1".to_string(),
            payload: serde_json::json!({"value": "too large"}),
        };
        let response = query_handler(
            axum::extract::State(state),
            axum::http::HeaderMap::new(),
            Json(request),
        )
        .await
        .into_response();
        assert_eq!(response.status().as_u16(), 413);
    });
}

#[test]
fn query_handler_dispatches_app_query_router() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    assert!(runtime.is_ok());
    let runtime = match runtime {
        Ok(runtime) => runtime,
        Err(_) => return,
    };
    runtime.block_on(async {
        let mut app_router = ApiRouter::new();
        assert!(app_router.register_query(AppEchoQuery).is_ok());
        let state = HttpState {
            static_info: test_snapshot(),
            sync_log: None,
            tick_counter: None,
            operation_mode: None,
            supervisor: None,
            command_policy: None,
            controller: None,
            app_query_router: Some(Arc::new(Mutex::new(app_router))),
            auth: super::HttpCommandAuth::insecure_local_for_testing(),
            max_payload_bytes: 65_536,
            clock: Arc::new(appcore_core::SystemClock::new()),
        };
        let request = QueryRequest {
            query_name: "note.get".to_string(),
            query_id: "qry-1".to_string(),
            payload: serde_json::json!({"id": "n1"}),
        };
        let response = query_handler(
            axum::extract::State(state),
            axum::http::HeaderMap::new(),
            Json(request),
        )
        .await
        .into_response();
        assert_eq!(response.status().as_u16(), 200);
        let body = to_bytes(response.into_body(), usize::MAX).await;
        assert!(body.is_ok());
        let parsed = serde_json::from_slice::<QueryResponse>(&body.unwrap());
        assert!(parsed.is_ok());
        let parsed = parsed.unwrap();
        assert!(parsed.ok);
        assert_eq!(parsed.payload["id"], "n1");
    });
}

#[test]
fn query_handler_authorizes_application_capability_before_dispatch() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut app_router = ApiRouter::new();
        app_router.register_query(AppEchoQuery).unwrap();
        let state = HttpState {
            static_info: test_snapshot(),
            sync_log: None,
            tick_counter: None,
            operation_mode: None,
            supervisor: None,
            command_policy: Some(Arc::new(RejectQueryPolicy)),
            controller: None,
            app_query_router: Some(Arc::new(Mutex::new(app_router))),
            auth: super::HttpCommandAuth::insecure_local_for_testing(),
            max_payload_bytes: 65_536,
            clock: Arc::new(appcore_core::SystemClock::new()),
        };
        let response = query_handler(
            axum::extract::State(state),
            axum::http::HeaderMap::new(),
            Json(QueryRequest {
                query_name: "note.get".to_string(),
                query_id: "qry-policy".to_string(),
                payload: serde_json::json!({"id": "n1"}),
            }),
        )
        .await
        .into_response();

        assert_eq!(response.status(), axum::http::StatusCode::FORBIDDEN);
    });
}

#[test]
fn query_is_audited_with_trace_and_latency() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let controller = Arc::new(Mutex::new(build_controller().unwrap()));
        let state = HttpState {
            static_info: test_snapshot(),
            sync_log: None,
            tick_counter: None,
            operation_mode: None,
            supervisor: None,
            command_policy: None,
            controller: Some(Arc::clone(&controller)),
            app_query_router: None,
            auth: super::HttpCommandAuth::insecure_local_for_testing(),
            max_payload_bytes: 65_536,
            clock: Arc::new(appcore_core::SystemClock::new()),
        };
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            "x-appcore-trace-id",
            HeaderValue::from_static("trace-query-1"),
        );
        let request = QueryRequest {
            query_name: "runtime.status".to_string(),
            query_id: "query-traced".to_string(),
            payload: serde_json::json!({}),
        };

        let response = query_handler(axum::extract::State(state), headers, Json(request)).await;
        assert_eq!(
            response.into_response().status(),
            axum::http::StatusCode::OK
        );
        let entries = controller.lock().instance().audit_log().entries();
        let entry = entries
            .iter()
            .find(|entry| entry.category == AuditCategory::Query)
            .unwrap();
        assert_eq!(entry.operation_id, "query-traced");
        assert_eq!(
            entry.trace.as_ref().map(|trace| trace.trace_id.as_str()),
            Some("trace-query-1")
        );
        assert_eq!(
            entry.latency_ms,
            entry.completed_at_ms.saturating_sub(entry.started_at_ms)
        );
    });
}

#[test]
fn health_handler_is_public() {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => return,
    };
    runtime.block_on(async {
        let response = super::health_handler(axum::extract::State(status_state(true))).await;
        assert_eq!(response.status().as_u16(), 200);
    });
}

#[test]
fn test_authorize_status_semantics() {
    use super::auth::authorize_status;
    use super::HttpCommandAuth;
    use axum::http::{HeaderMap, HeaderValue, StatusCode};

    // 1. public_status = true, no token
    let auth_public = HttpCommandAuth {
        require_token: false,
        public_status: true,
        verifier: None,
    };
    let headers = HeaderMap::new();
    assert_eq!(authorize_status(&auth_public, &headers), Ok(false));

    // 2. public_status = false, no token
    let auth_protected = HttpCommandAuth {
        require_token: false,
        public_status: false,
        verifier: None,
    };
    assert_eq!(
        authorize_status(&auth_protected, &headers),
        Err(StatusCode::UNAUTHORIZED)
    );

    // 3. public_status = false, invalid token
    let auth_protected_deny = HttpCommandAuth {
        require_token: false,
        public_status: false,
        verifier: Some(Arc::new(DenyVerifier)),
    };
    let mut headers_token = HeaderMap::new();
    headers_token.insert("authorization", HeaderValue::from_static("Bearer bad"));
    assert_eq!(
        authorize_status(&auth_protected_deny, &headers_token),
        Err(StatusCode::UNAUTHORIZED)
    );

    // 4. public_status = false, valid token
    let auth_protected_allow = HttpCommandAuth {
        require_token: false,
        public_status: false,
        verifier: Some(Arc::new(AllowVerifier)),
    };
    assert_eq!(
        authorize_status(&auth_protected_allow, &headers_token),
        Ok(true)
    );

    let mut duplicate_headers = headers_token;
    duplicate_headers.append("authorization", HeaderValue::from_static("Bearer second"));
    assert_eq!(
        authorize_status(&auth_protected_allow, &duplicate_headers),
        Err(StatusCode::UNAUTHORIZED)
    );
}
