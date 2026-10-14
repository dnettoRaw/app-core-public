// =============================================================================
//        #######
//     ###       ###     F: diagnostic.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/12 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/12 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Safe projection of validated wire rejection metadata for operators.
use super::{
    PeerRpcWireErrorCodeV2 as Code, PeerRpcWireErrorV2, PeerRpcWireErrorValidationErrorV2,
};
use appcore_types::CoreId;
use serde::Serialize;

/// Protocol-owned suggestion. It never authorizes an automatic command replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PeerRpcDiagnosticActionV2 {
    /// Retry only a fresh safely idempotent higher-level operation under its budget.
    RetryBoundedOperation,
    /// Check local authentication/policy without exposing credential detail remotely.
    ReviewLocalPolicy,
    /// Check deployment capability registration and worker health.
    CheckCapability,
    /// Check declared protocol, target and version compatibility.
    CheckCompatibility,
    /// Obtain authenticated current history or a snapshot.
    RefreshHistory,
    /// Check local clock/window; expiry alone is not proof of clock skew.
    CheckDeadlineAndClock,
    /// Inspect integrity/identity evidence locally; never skip envelope validation.
    InspectLocalEvidence,
    /// A future/unknown error requires local investigation with no automatic retry.
    ManualReview,
}
/// Path/payload/credential-free operator evidence from a validated wire error.
/// Tenant authorization remains the deployment's export boundary.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct PeerRpcDiagnosticV2 {
    schema_version: u16,
    peer: CoreId,
    error: PeerRpcWireErrorV2,
    action: PeerRpcDiagnosticActionV2,
}
impl PeerRpcDiagnosticV2 {
    /// Preserves the exact controlled code, phase and correlation through layers.
    /// Unknown codes retain the existing terminal, redacted wire normalization.
    pub fn from_wire(
        peer: CoreId,
        error: PeerRpcWireErrorV2,
    ) -> Result<Self, PeerRpcWireErrorValidationErrorV2> {
        let error = error.validated()?;
        let action = recommended_action(error.code);
        Ok(Self {
            schema_version: 2,
            peer,
            error,
            action,
        })
    }
    /// Exact validated wire error; free-form remote details are never retained.
    pub fn error(&self) -> &PeerRpcWireErrorV2 {
        &self.error
    }
    /// Protocol-owned suggestion; caller still owns operation idempotency/retry policy.
    pub fn action(&self) -> PeerRpcDiagnosticActionV2 {
        self.action
    }
}
impl std::fmt::Debug for PeerRpcDiagnosticV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PeerRpcDiagnosticV2")
            .field("error", &self.error)
            .field("action", &self.action)
            .finish_non_exhaustive()
    }
}
fn recommended_action(code: Code) -> PeerRpcDiagnosticActionV2 {
    use PeerRpcDiagnosticActionV2 as Action;
    match code {
        Code::Unknown => Action::ManualReview,
        Code::Unauthorized | Code::Forbidden => Action::ReviewLocalPolicy,
        Code::CapabilityNotFound => Action::CheckCapability,
        Code::Incompatible | Code::ProtocolMismatch => Action::CheckCompatibility,
        Code::Stale => Action::RefreshHistory,
        Code::Expired => Action::CheckDeadlineAndClock,
        code if code.retryable() => Action::RetryBoundedOperation,
        _ => Action::InspectLocalEvidence,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn correlation_survives_and_unknown_details_do_not() {
        let peer = CoreId::new("peer-a").unwrap();
        let error =
            PeerRpcWireErrorV2::controlled(Some("correlation-a".into()), None, Code::Expired);
        let diagnostic = PeerRpcDiagnosticV2::from_wire(peer.clone(), error.clone()).unwrap();
        assert_eq!(diagnostic.error(), &error);
        assert_eq!(
            diagnostic.action(),
            PeerRpcDiagnosticActionV2::CheckDeadlineAndClock
        );
        assert!(serde_json::to_string(&diagnostic)
            .unwrap()
            .contains("correlation-a"));
        let mut unknown = PeerRpcWireErrorV2::controlled(None, None, Code::Unknown);
        unknown.message = "private-marker".into();
        unknown.retryable = true;
        let diagnostic = PeerRpcDiagnosticV2::from_wire(peer, unknown).unwrap();
        assert!(!serde_json::to_string(&diagnostic)
            .unwrap()
            .contains("private-marker"));
        assert!(!diagnostic.error().retryable);
    }
}
