// =============================================================================
//        #######
//     ###       ###     F: trace.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Lightweight distributed correlation context.

use crate::error::RuntimeResult;
use crate::ids::{validate_identifier, CoreId, TenantId};

/// Correlation context propagated across distributed Runtime boundaries.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TraceContext {
    /// End-to-end trace identity.
    pub trace_id: String,
    /// Current span identity.
    pub span_id: String,
    /// Parent span identity, when this is a child span.
    pub parent_span_id: Option<String>,
    /// Core that originated the trace.
    pub originating_core_id: CoreId,
    /// Core currently handling the operation.
    pub current_core_id: CoreId,
    /// Tenant isolation boundary.
    pub tenant_id: TenantId,
    /// Optional command identity associated with the trace.
    pub command_id: Option<String>,
}

impl TraceContext {
    /// Creates and validates a root trace context.
    pub fn new(
        trace_id: impl Into<String>,
        span_id: impl Into<String>,
        originating_core_id: CoreId,
        current_core_id: CoreId,
        tenant_id: TenantId,
    ) -> RuntimeResult<Self> {
        let trace_id = trace_id.into();
        let span_id = span_id.into();
        validate_identifier("TraceId", &trace_id)?;
        validate_identifier("SpanId", &span_id)?;
        Ok(Self {
            trace_id,
            span_id,
            parent_span_id: None,
            originating_core_id,
            current_core_id,
            tenant_id,
            command_id: None,
        })
    }

    /// Associates a validated command identity with this context.
    pub fn with_command_id(mut self, command_id: impl Into<String>) -> RuntimeResult<Self> {
        let command_id = command_id.into();
        validate_identifier("CommandId", &command_id)?;
        self.command_id = Some(command_id);
        Ok(self)
    }

    /// Creates a child span for another Core while preserving correlation.
    pub fn child_span(
        &self,
        span_id: impl Into<String>,
        current_core_id: CoreId,
    ) -> RuntimeResult<Self> {
        let span_id = span_id.into();
        validate_identifier("SpanId", &span_id)?;
        Ok(Self {
            trace_id: self.trace_id.clone(),
            span_id,
            parent_span_id: Some(self.span_id.clone()),
            originating_core_id: self.originating_core_id.clone(),
            current_core_id,
            tenant_id: self.tenant_id.clone(),
            command_id: self.command_id.clone(),
        })
    }
}
