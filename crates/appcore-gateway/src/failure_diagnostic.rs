// =============================================================================
//        #######
//     ###       ###     F: failure_diagnostic.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/12 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/12 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Controlled Gateway route evidence without raw provider/authentication detail.
use crate::{GatewayError, WorkerSelectionError};
use serde::Serialize;

/// Stable local Gateway failure categories; no route or protocol alias is added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum GatewayFailureCodeV1 {
    /// Invalid deployment configuration.
    Configuration,
    /// Authentication rejected; sensitive cause remains local.
    Authentication,
    /// Authenticated caller lacks authorization.
    Authorization,
    /// Authenticated partition identity does not match.
    TenantMismatch,
    /// Coarse routing evidence does not distinguish absent/unhealthy workers.
    WorkerUnavailable,
    /// No worker advertises the selected capability.
    WorkerAbsent,
    /// Known workers are disconnected or unhealthy.
    WorkerUnhealthy,
    /// Healthy workers are at their explicit capacity limit.
    CapacityExceeded,
    /// Route transport is unavailable; acknowledgement may be ambiguous.
    TransportUnavailable,
    /// Bounded request serialization failed.
    InvalidPayload,
    /// Selected protocol or envelope is invalid.
    Protocol,
    /// Underlying Runtime rejected the operation; inspect local typed evidence.
    Runtime,
}
/// Safe route diagnostic. Tenant authorization remains the exporter's boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GatewayFailureDiagnosticV1 {
    schema_version: u16,
    code: GatewayFailureCodeV1,
    correlation_id: Option<String>,
    retryable: bool,
}
impl GatewayFailureDiagnosticV1 {
    /// Preserves a typed worker selection outcome before it is flattened to text.
    /// A retry hint never permits replay of an ambiguously acknowledged command.
    pub fn from_selection(error: &WorkerSelectionError, correlation: Option<&str>) -> Self {
        use GatewayFailureCodeV1 as Code;
        let code = match error {
            WorkerSelectionError::CapabilityUnavailable => Code::WorkerAbsent,
            WorkerSelectionError::NoHealthyWorker => Code::WorkerUnhealthy,
            WorkerSelectionError::AtCapacity => Code::CapacityExceeded,
            WorkerSelectionError::InvalidAffinity | WorkerSelectionError::InvalidLimits => {
                Code::Configuration
            }
        };
        Self::new(code, correlation)
    }
    fn new(code: GatewayFailureCodeV1, correlation: Option<&str>) -> Self {
        Self {
            schema_version: 1,
            code,
            correlation_id: correlation
                .filter(|id| {
                    !id.is_empty()
                        && id.len() <= 128
                        && id.bytes().all(|byte| {
                            byte.is_ascii_alphanumeric()
                                || matches!(byte, b'.' | b'_' | b'-' | b':')
                        })
                })
                .map(str::to_owned),
            retryable: matches!(
                code,
                GatewayFailureCodeV1::WorkerUnhealthy
                    | GatewayFailureCodeV1::CapacityExceeded
                    | GatewayFailureCodeV1::TransportUnavailable
            ),
        }
    }
    /// Stable code, suitable for operational projection/UI classification.
    pub fn code(&self) -> GatewayFailureCodeV1 {
        self.code
    }
    /// Whether an independently safe higher-level operation may retry under a budget.
    pub fn retryable(&self) -> bool {
        self.retryable
    }
}
impl GatewayError {
    /// Produces controlled evidence without parsing or exporting raw error strings.
    pub fn diagnostic_v1(&self, correlation: Option<&str>) -> GatewayFailureDiagnosticV1 {
        use GatewayFailureCodeV1 as Code;
        let code = match self {
            Self::Config(_) => Code::Configuration,
            Self::Authentication(_) => Code::Authentication,
            Self::Forbidden(_) => Code::Authorization,
            Self::TenantMismatch(_) => Code::TenantMismatch,
            Self::WorkerUnavailable(_) => Code::WorkerUnavailable,
            Self::Transport(_) => Code::TransportUnavailable,
            Self::Serialization(_) => Code::InvalidPayload,
            Self::Protocol(_) => Code::Protocol,
            Self::Runtime(_) => Code::Runtime,
        };
        GatewayFailureDiagnosticV1::new(code, correlation)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_selection_and_correlation_without_raw_error_detail() {
        let absent = GatewayFailureDiagnosticV1::from_selection(
            &WorkerSelectionError::CapabilityUnavailable,
            Some("request-a"),
        );
        assert_eq!(absent.code(), GatewayFailureCodeV1::WorkerAbsent);
        assert!(!absent.retryable());
        let busy =
            GatewayFailureDiagnosticV1::from_selection(&WorkerSelectionError::AtCapacity, None);
        assert!(busy.retryable());
        let diagnostic =
            GatewayError::Authentication("private-marker".into()).diagnostic_v1(Some("request-a"));
        let encoded = serde_json::to_string(&diagnostic).unwrap();
        assert!(encoded.contains("request-a"));
        assert!(!encoded.contains("private-marker"));
    }
}
