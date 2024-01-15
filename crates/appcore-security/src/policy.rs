// =============================================================================
//        #######
//     ###       ###     F: policy.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/31 13:38:42 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Authorization policy contracts for runtime commands and internal actions.

use appcore_core::CommandName;

/// Policy decision outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    /// Policy permits execution.
    Allow,
    /// Policy rejects execution with a controlled reason.
    Deny(String),
}

/// Contract for policy checks.
pub trait PolicyCheck {
    /// Returns the command governed by this policy.
    fn command_name(&self) -> &CommandName;
    /// Evaluates the policy.
    fn evaluate(&self) -> PolicyDecision;
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;
