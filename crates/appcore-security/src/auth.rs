// =============================================================================
//        #######
//     ###       ###     F: auth.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/31 13:38:42 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Authentication contracts for runtime command and peer entry points.

use appcore_core::{AppId, CommandName, NodeId};

/// Authentication request context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthContext {
    /// Application scope.
    pub app_id: AppId,
    /// Runtime node scope.
    pub node_id: NodeId,
    /// Command being authenticated.
    pub command_name: CommandName,
}

/// Authentication decision outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthDecision {
    /// Authentication succeeded.
    Allow,
    /// Authentication failed with a controlled reason.
    Deny(String),
}

/// Contract for runtime authenticators.
pub trait Authenticator {
    /// Authenticates one Runtime request context.
    fn authenticate(&self, context: &AuthContext) -> AuthDecision;
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
