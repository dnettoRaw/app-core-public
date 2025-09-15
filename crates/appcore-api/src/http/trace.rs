// =============================================================================
//        #######
//     ###       ###     F: trace.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/21 10:48:21 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/21 10:48:21 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! HTTP ingress trace construction and validation.

use super::state::RuntimeStaticInfo;
use appcore_core::{CoreId, TenantId, TraceContext};
use axum::http::HeaderMap;

pub(crate) fn request_trace(
    headers: &HeaderMap,
    operation_id: &str,
    static_info: &RuntimeStaticInfo,
) -> Result<TraceContext, ()> {
    let trace_id = optional_header(headers, "x-appcore-trace-id")?
        .unwrap_or(operation_id)
        .to_string();
    let span_id = optional_header(headers, "x-appcore-span-id")?
        .unwrap_or(operation_id)
        .to_string();
    let core_id = CoreId::new(static_info.core_id.clone()).map_err(|_| ())?;
    let tenant_id = TenantId::new(static_info.tenant_id.clone()).map_err(|_| ())?;
    TraceContext::new(trace_id, span_id, core_id.clone(), core_id, tenant_id).map_err(|_| ())
}

fn optional_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, ()> {
    headers
        .get(name)
        .map(|value| value.to_str().map_err(|_| ()))
        .transpose()
}
