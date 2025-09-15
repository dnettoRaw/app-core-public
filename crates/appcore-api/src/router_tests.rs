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
