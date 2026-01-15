// =============================================================================
//        #######
//     ###       ###     F: session.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/26 08:53:09 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/26 08:53:09 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Session management and token context tracking.

use appcore_types::TenantId;

/// Represents an authenticated client session.
#[derive(Debug, Clone)]
pub struct GatewaySession {
    /// Unique identifier for this session.
    pub session_id: String,

    /// Tenant scope constraint.
    pub tenant_id: TenantId,

    /// When the session was established (Unix epoch ms).
    pub created_at_ms: u64,

    /// When the session token expires (Unix epoch ms).
    pub expires_at_ms: u64,

    /// Subject derived from client credentials.
    pub subject: Option<String>,
}

impl GatewaySession {
    /// Creates a new gateway session.
    pub fn new(
        session_id: String,
        tenant_id: TenantId,
        created_at_ms: u64,
        expires_at_ms: u64,
        subject: Option<String>,
    ) -> Self {
        Self {
            session_id,
            tenant_id,
            created_at_ms,
            expires_at_ms,
            subject,
        }
    }

    /// Reports whether the session has expired relative to `now_ms`.
    pub fn is_expired(&self, now_ms: u64) -> bool {
        now_ms >= self.expires_at_ms
    }
}
