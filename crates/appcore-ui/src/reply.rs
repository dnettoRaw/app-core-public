// =============================================================================
//        #######
//     ###       ###     F: reply.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Controlled correlated replies and ephemeral event notifications.
use crate::{UiConnectionState, UiError, UiResult, UI_PROTOCOL_V1};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
/// One correlated result of a UI V1 action.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiReplyV1 {
    /// Explicit UI wire family version.
    pub version: u16,
    /// Host-bound surface identity.
    pub surface_id: String,
    /// Caller request identity.
    pub request_id: String,
    /// Transport-link state, independent of application/peer health.
    pub state: UiConnectionState,
    /// An exclusive success/error outcome without free-form error messages.
    pub outcome: UiOutcomeV1,
}
/// An exclusive wire result; unknown codes or contradictory envelopes are not inferred.
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum UiOutcomeV1 {
    /// A bounded serializable success value.
    Ok {
        /// Application result or protocol-owned control result.
        value: Value,
    },
    /// A controlled infrastructure error.
    Error {
        /// Stable error code.
        code: UiError,
    },
}
impl UiReplyV1 {
    pub(crate) fn new(
        surface_id: &str,
        request_id: &str,
        state: UiConnectionState,
        result: UiResult<Value>,
    ) -> Self {
        Self {
            version: UI_PROTOCOL_V1,
            surface_id: surface_id.to_string(),
            request_id: request_id.to_string(),
            state,
            outcome: match result {
                Ok(value) => UiOutcomeV1::Ok { value },
                Err(code) => UiOutcomeV1::Error { code },
            },
        }
    }
}
impl fmt::Debug for UiReplyV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UiReplyV1")
            .field("version", &self.version)
            .field("surface_id", &self.surface_id)
            .field("request_id", &self.request_id)
            .field("state", &self.state)
            .field("outcome", &"REDACTED")
            .finish()
    }
}
/// One bounded ephemeral UI event, not an application durable outbox record.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiEventV1 {
    /// Safe-integer sequence, monotonically increasing within one surface instance.
    pub sequence: u64,
    /// Explicit declared event topic.
    pub topic: String,
    /// Application-owned opaque notification content.
    pub payload: Value,
}
impl fmt::Debug for UiEventV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UiEventV1")
            .field("sequence", &self.sequence)
            .field("topic", &self.topic)
            .field("payload", &"REDACTED")
            .finish()
    }
}
