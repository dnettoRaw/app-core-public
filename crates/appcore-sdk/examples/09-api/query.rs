// =============================================================================
//        #######
//     ###       ###     F: query.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/04 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/04 00:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

//! Register and execute a transport-neutral query without starting a listener.

use appcore_sdk::application::{
    ApiMethod, ApiRequest, ApiResponse, ApiRouter, NodeId, QueryEndpoint, QueryName, RuntimeResult,
};
use appcore_sdk::{App, AppResult, Application};

struct StatusApplication;

struct StatusQuery {
    name: QueryName,
}

impl QueryEndpoint for StatusQuery {
    fn query_name(&self) -> &QueryName {
        &self.name
    }

    fn handle_query(&self, request: ApiRequest) -> RuntimeResult<ApiResponse> {
        Ok(ApiResponse {
            status_code: 200,
            payload: request.payload,
        })
    }
}

impl Application for StatusApplication {
    fn register_queries(&self, router: &mut ApiRouter) -> RuntimeResult<()> {
        router.register_query(StatusQuery {
            name: QueryName::new("status.read")?,
        })
    }
}

fn main() -> AppResult<()> {
    let app = App::new("query-example")?;
    let prepared = app.prepare(&StatusApplication, NodeId::new("query-node")?)?;
    let name = QueryName::new("status.read")?;
    let response = prepared.queries().dispatch_query(
        &name,
        ApiRequest {
            method: ApiMethod::Query,
            path: "/v1/query/status.read".to_string(),
            payload: b"ready".to_vec(),
        },
    )?;

    assert_eq!(response.payload, b"ready");
    Ok(())
}
