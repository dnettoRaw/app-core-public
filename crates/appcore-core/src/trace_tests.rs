// =============================================================================
//        #######
//     ###       ###     F: trace_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/20 23:03:47 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/20 23:03:47 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::TraceContext;
use crate::{CoreId, TenantId};

#[test]
fn child_span_preserves_trace_and_parent() {
    let trace = TraceContext::new(
        "trace-1",
        "span-1",
        CoreId::new("core-a").unwrap(),
        CoreId::new("core-a").unwrap(),
        TenantId::new("tenant-a").unwrap(),
    )
    .unwrap()
    .with_command_id("cmd-1")
    .unwrap();
    let child = trace
        .child_span("span-2", CoreId::new("core-b").unwrap())
        .unwrap();
    assert_eq!(child.trace_id, "trace-1");
    assert_eq!(child.parent_span_id.as_deref(), Some("span-1"));
    assert_eq!(child.command_id.as_deref(), Some("cmd-1"));
    assert_eq!(child.current_core_id.as_str(), "core-b");
}
