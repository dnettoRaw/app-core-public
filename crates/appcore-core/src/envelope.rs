// =============================================================================
//        #######
//     ###       ###     F: envelope.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/29 20:47:35 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Transport-neutral command/event envelope contracts.
//! These are passed to command handlers and returned in command results.

use crate::error::RuntimeResult;
use crate::ids::{validate_identifier, AppId, CommandName, EventName, NodeId};
use crate::trace::TraceContext;

/// Immutable command envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandEnvelope {
    /// Stable command name.
    pub command_name: CommandName,
    /// Unique command identity.
    pub command_id: String,
    /// Application issuing the command.
    pub app_id: AppId,
    /// Node issuing the command.
    pub node_id: NodeId,
    /// Issue timestamp in Unix milliseconds.
    pub issued_at_ms: u64,
    /// Optional key used to deduplicate mutating commands.
    pub idempotency_key: Option<String>,
    /// Opaque application-owned payload.
    pub payload: Vec<u8>,
    /// Optional distributed trace context.
    pub trace: Option<TraceContext>,
}

impl CommandEnvelope {
    /// Creates and validates a command envelope.
    pub fn new(
        command_name: CommandName,
        command_id: String,
        app_id: AppId,
        node_id: NodeId,
        issued_at_ms: u64,
        idempotency_key: Option<String>,
        payload: Vec<u8>,
    ) -> RuntimeResult<Self> {
        validate_identifier("CommandId", &command_id)?;
        if let Some(key) = &idempotency_key {
            validate_identifier("IdempotencyKey", key)?;
        }
        command_name.validate()?;
        app_id.validate()?;
        node_id.validate()?;

        Ok(Self {
            command_name,
            command_id,
            app_id,
            node_id,
            issued_at_ms,
            idempotency_key,
            payload,
            trace: None,
        })
    }

    /// Attaches distributed trace context.
    pub fn with_trace(mut self, trace: TraceContext) -> Self {
        self.trace = Some(trace);
        self
    }

    /// Returns the command name.
    pub fn command_name(&self) -> &CommandName {
        &self.command_name
    }

    /// Returns opaque command payload bytes.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

/// Immutable event envelope.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EventEnvelope {
    /// Stable event name.
    pub event_name: EventName,
    /// Unique event identity.
    pub event_id: String,
    /// Application that emitted the event.
    pub app_id: AppId,
    /// Node that emitted the event.
    pub node_id: NodeId,
    /// Occurrence timestamp in Unix milliseconds.
    pub occurred_at_ms: u64,
    /// Opaque application-owned payload.
    pub payload: Vec<u8>,
    /// Optional distributed trace context.
    pub trace: Option<TraceContext>,
}

impl EventEnvelope {
    /// Creates and validates an event envelope.
    pub fn new(
        event_name: EventName,
        event_id: String,
        app_id: AppId,
        node_id: NodeId,
        occurred_at_ms: u64,
        payload: Vec<u8>,
    ) -> RuntimeResult<Self> {
        validate_identifier("EventId", &event_id)?;
        event_name.validate()?;
        app_id.validate()?;
        node_id.validate()?;

        Ok(Self {
            event_name,
            event_id,
            app_id,
            node_id,
            occurred_at_ms,
            payload,
            trace: None,
        })
    }

    /// Attaches distributed trace context.
    pub fn with_trace(mut self, trace: TraceContext) -> Self {
        self.trace = Some(trace);
        self
    }

    /// Returns the event name.
    pub fn event_name(&self) -> &EventName {
        &self.event_name
    }

    /// Returns opaque event payload bytes.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

#[cfg(test)]
#[path = "envelope_tests.rs"]
mod tests;
