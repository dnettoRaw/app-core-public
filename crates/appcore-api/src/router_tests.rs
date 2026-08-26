// =============================================================================
//        #######
//     ###       ###     F: router_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 13:45:20 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use appcore_core::RuntimeResult;

use super::ApiRouter;
use crate::api::{ApiMethod, ApiRequest, ApiResponse};
use crate::command_endpoint::CommandEndpoint;
use crate::query_endpoint::{QueryEndpoint, QueryName};
use parking_lot::{Condvar, Mutex};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

struct RuntimeTestQuery;

impl QueryEndpoint for RuntimeTestQuery {
    fn query_name(&self) -> &QueryName {
        static NAME: std::sync::OnceLock<QueryName> = std::sync::OnceLock::new();
        NAME.get_or_init(|| QueryName::new("runtime.test".to_string()).unwrap())
    }

    fn handle_query(&self, _request: ApiRequest) -> RuntimeResult<ApiResponse> {
        Ok(ApiResponse {
            status_code: 200,
            payload: vec![9],
        })
    }
}

struct CommandEp;

impl CommandEndpoint for CommandEp {
    fn handle_command(&self, _request: ApiRequest) -> RuntimeResult<ApiResponse> {
        Ok(ApiResponse {
            status_code: 202,
            payload: vec![7],
        })
    }
}

fn query_request() -> ApiRequest {
    ApiRequest {
        method: ApiMethod::Query,
        path: "/api/query/runtime.test".to_string(),
        payload: vec![],
    }
}

fn command_request() -> ApiRequest {
    ApiRequest {
        method: ApiMethod::Command,
        path: "/api/command".to_string(),
        payload: vec![],
    }
}

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn api_router_can_be_shared_by_http_state() {
    assert_send_sync::<ApiRouter>();
}

#[test]
fn registra_query() {
    let mut router = ApiRouter::new();
    assert!(router.register_query(RuntimeTestQuery).is_ok());
}

#[test]
fn rejeita_query_duplicada() {
    let mut router = ApiRouter::new();
    assert!(router.register_query(RuntimeTestQuery).is_ok());
    assert!(router.register_query(RuntimeTestQuery).is_err());
}

#[test]
fn dispatch_query_chama_endpoint() {
    let mut router = ApiRouter::new();
    assert!(router.register_query(RuntimeTestQuery).is_ok());
    let response = router.dispatch_query(
        &QueryName::new("runtime.test".to_string()).unwrap(),
        query_request(),
    );
    assert!(response.is_ok());
    let response = match response {
        Ok(response) => response,
        Err(_) => return,
    };
    assert_eq!(response.status_code, 200);
}

#[test]
fn dispatch_query_ausente_retorna_erro() {
    let router = ApiRouter::new();
    let response = router.dispatch_query(
        &QueryName::new("missing".to_string()).unwrap(),
        query_request(),
    );
    assert!(response.is_err());
}

#[test]
fn dispatch_command_sem_endpoint_retorna_erro() {
    let router = ApiRouter::new();
    let response = router.dispatch_command(command_request());
    assert!(response.is_err());
}

#[test]
fn dispatch_command_chama_endpoint() {
    let mut router = ApiRouter::new();
    router.set_command_endpoint(CommandEp);
    let response = router.dispatch_command(command_request());
    assert!(response.is_ok());
    let response = match response {
        Ok(response) => response,
        Err(_) => return,
    };
    assert_eq!(response.status_code, 202);
}

#[test]
fn has_query_funciona() {
    let mut router = ApiRouter::new();
    assert!(router.register_query(RuntimeTestQuery).is_ok());
    assert!(router.has_query(&QueryName::new("runtime.test".to_string()).unwrap()));
    assert_eq!(
        router.query_names(),
        vec![QueryName::new("runtime.test".to_string()).unwrap()]
    );
}

#[test]
fn frozen_query_registry_rejects_late_registration() {
    let mut router = ApiRouter::new();
    assert!(router.register_query(RuntimeTestQuery).is_ok());
    router.freeze_queries();
    let snapshot = router.clone();

    assert!(router.queries_are_frozen());
    assert!(snapshot.queries_are_frozen());
    assert!(matches!(
        router.register_query(RuntimeTestQuery),
        Err(appcore_core::RuntimeError::InvalidRequest {
            kind: "query",
            reason: "router_frozen"
        })
    ));
    assert_eq!(router.query_names().len(), 1);
    assert_eq!(snapshot.query_names(), router.query_names());
}

#[derive(Default)]
struct QueryProbe {
    state: Mutex<QueryProbeState>,
    changed: Condvar,
}

#[derive(Default)]
struct QueryProbeState {
    entered: usize,
    active: usize,
    peak: usize,
    released: bool,
}

impl QueryProbe {
    fn enter(&self) {
        let started = Instant::now();
        let mut state = self.state.lock();
        state.entered += 1;
        state.active += 1;
        state.peak = state.peak.max(state.active);
        self.changed.notify_all();
        while !state.released {
            let remaining = Duration::from_secs(5).saturating_sub(started.elapsed());
            if remaining.is_zero() {
                break;
            }
            self.changed.wait_for(&mut state, remaining);
        }
        state.active -= 1;
    }

    fn wait_for_entered(&self, expected: usize, timeout: Duration) -> bool {
        let started = Instant::now();
        let mut state = self.state.lock();
        while state.entered < expected {
            let remaining = timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return false;
            }
            self.changed.wait_for(&mut state, remaining);
        }
        true
    }

    fn release(&self) {
        let mut state = self.state.lock();
        state.released = true;
        self.changed.notify_all();
    }

    fn peak(&self) -> usize {
        self.state.lock().peak
    }
}

struct ConcurrentQuery {
    name: QueryName,
    probe: Arc<QueryProbe>,
}

impl QueryEndpoint for ConcurrentQuery {
    fn query_name(&self) -> &QueryName {
        &self.name
    }

    fn handle_query(&self, _request: ApiRequest) -> RuntimeResult<ApiResponse> {
        self.probe.enter();
        Ok(ApiResponse {
            status_code: 200,
            payload: Vec::new(),
        })
    }
}

#[test]
fn frozen_router_snapshots_dispatch_queries_concurrently() {
    const WORKERS: usize = 8;
    let probe = Arc::new(QueryProbe::default());
    let name = QueryName::new("runtime.concurrent").unwrap();
    let mut router = ApiRouter::new();
    router
        .register_query(ConcurrentQuery {
            name: name.clone(),
            probe: Arc::clone(&probe),
        })
        .unwrap();
    router.freeze_queries();
    let barrier = Arc::new(Barrier::new(WORKERS + 1));
    let mut workers = Vec::new();

    for _ in 0..WORKERS {
        let router = router.clone();
        let name = name.clone();
        let barrier = Arc::clone(&barrier);
        workers.push(thread::spawn(move || {
            barrier.wait();
            router.dispatch_query(&name, query_request())
        }));
    }

    barrier.wait();
    assert!(probe.wait_for_entered(WORKERS, Duration::from_secs(2)));
    assert_eq!(probe.peak(), WORKERS);
    probe.release();
    for worker in workers {
        assert_eq!(worker.join().unwrap().unwrap().status_code, 200);
    }
}
