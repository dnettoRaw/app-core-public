// =============================================================================
//        #######
//     ###       ###     F: protocol.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! UI wire V1 composes existing command/query DTOs without changing their contracts.
use crate::{CommandRequest, QueryRequest};
use serde::{Deserialize, Serialize};
use std::fmt;
/// Explicit version of this UI wire family, independent of Peer RPC.
pub const UI_PROTOCOL_V1: u16 = 1;
/// Transport-link state, distinct from application or peer health.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiConnectionState {
    /// The bridge can serve authorized operations.
    Connected,
    /// The selected transport is not reachable.
    Offline,
    /// The bridge is reachable but its owning infrastructure is degraded.
    Degraded,
    /// The surface has been closed and must be recreated explicitly.
    Closed,
}
/// One versioned frame; host identity and credentials are never client fields.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiRequestV1 {
    /// Explicit UI wire version.
    pub version: u16,
    /// Surface identity bound to the native/transport caller by the adapter.
    pub surface_id: String,
    /// Unique in-flight correlation identity.
    pub request_id: String,
    /// Monotonic execution budget starting at host admission.
    pub timeout_ms: u32,
    /// Requested operation with application-owned opaque content.
    pub action: UiActionV1,
}
/// Versioned actions; the page cannot publish events or grant itself capabilities.
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum UiActionV1 {
    /// Dispatch the existing V1 side-effect-free query contract.
    Query {
        /// Existing Runtime query.
        query: QueryRequest,
    },
    /// Dispatch the existing V1 command contract through its owning handler.
    Command {
        /// Existing Runtime command.
        command: CommandRequest,
    },
    /// Subscribe to a declared capability's permitted event topic.
    Subscribe {
        /// Capability used to authorize the subscription.
        capability: String,
        /// Bounded event topic.
        topic: String,
    },
    /// Pull a bounded event page, acknowledging removal from this UI queue.
    Poll {
        /// Subscription identity within this surface generation.
        subscription_id: String,
    },
    /// Release one subscription and all its retained notifications.
    Unsubscribe {
        /// Subscription identity.
        subscription_id: String,
    },
    /// Cooperatively cancel one operation; completed effects are not rolled back.
    Cancel {
        /// Original in-flight request identity.
        target_request_id: String,
    },
    /// Close this surface, cancelling work and releasing subscription queues.
    Close,
}
impl fmt::Debug for UiRequestV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UiRequestV1")
            .field("version", &self.version)
            .field("surface_id", &self.surface_id)
            .field("request_id", &self.request_id)
            .field("timeout_ms", &self.timeout_ms)
            .field("action", &self.action)
            .finish()
    }
}
impl fmt::Debug for UiActionV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Query { query } => f
                .debug_struct("Query")
                .field("name", &query.query_name)
                .field("payload", &"REDACTED")
                .finish(),
            Self::Command { command } => f
                .debug_struct("Command")
                .field("name", &command.command_name)
                .field("payload", &"REDACTED")
                .field("idempotency_key", &"REDACTED")
                .finish(),
            Self::Subscribe { capability, topic } => f
                .debug_struct("Subscribe")
                .field("capability", capability)
                .field("topic", topic)
                .finish(),
            Self::Poll { subscription_id } => f.debug_tuple("Poll").field(subscription_id).finish(),
            Self::Unsubscribe { subscription_id } => {
                f.debug_tuple("Unsubscribe").field(subscription_id).finish()
            }
            Self::Cancel { target_request_id } => {
                f.debug_tuple("Cancel").field(target_request_id).finish()
            }
            Self::Close => f.write_str("Close"),
        }
    }
}
