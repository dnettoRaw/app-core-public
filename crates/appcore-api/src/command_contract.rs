// =============================================================================
//        #######
//     ###       ###     F: command_contract.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/31 13:38:42 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 13:45:20 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Shared command request/response API contract for transports.

use appcore_core::{AppId, CommandEnvelope, CommandName, NodeId, RuntimeResult};
use serde::{Deserialize, Serialize};

/// Version 1 command request transported to the Runtime host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandRequest {
    /// Declared application or Runtime command capability.
    pub command_name: String,
    /// Caller-assigned request identity.
    pub command_id: String,
    /// Replay-safe identity required by mutating commands.
    pub idempotency_key: Option<String>,
    /// Opaque UTF-8 application payload.
    pub payload: String,
}

/// Event identity returned by an accepted command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandResponseEvent {
    /// Registered event name.
    pub event_name: String,
    /// Unique emitted event identity.
    pub event_id: String,
}

/// Version 1 controlled command response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandResponse {
    /// Whether the command was accepted.
    pub accepted: bool,
    /// Controlled rejection detail, when present.
    pub message: Option<String>,
    /// Events emitted by an accepted command.
    pub events: Vec<CommandResponseEvent>,
}

/// Validation failures defined by the command V1 contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandRequestValidationError {
    /// The command name is empty.
    EmptyCommandName,
    /// The command identifier is empty.
    EmptyCommandId,
    /// The payload exceeds the configured request bound.
    PayloadTooLarge,
    /// A mutating command omitted its idempotency key.
    MissingIdempotencyKey,
    /// The supplied idempotency key is malformed.
    InvalidIdempotencyKey,
    /// The command name is malformed.
    InvalidCommandName,
    /// The command identifier is malformed.
    InvalidCommandId,
}

impl CommandRequest {
    /// Validates identifiers, idempotency and the payload bound.
    pub fn validate(&self, max_payload_bytes: usize) -> Result<(), CommandRequestValidationError> {
        if self.command_name.trim().is_empty() {
            return Err(CommandRequestValidationError::EmptyCommandName);
        }
        if self.command_id.trim().is_empty() {
            return Err(CommandRequestValidationError::EmptyCommandId);
        }
        if self.command_name.len() > 128 || !is_valid_token(&self.command_name) {
            return Err(CommandRequestValidationError::InvalidCommandName);
        }
        if self.command_id.len() > 128 || !is_valid_token(&self.command_id) {
            return Err(CommandRequestValidationError::InvalidCommandId);
        }
        if requires_idempotency_key(&self.command_name) && self.idempotency_key.is_none() {
            return Err(CommandRequestValidationError::MissingIdempotencyKey);
        }
        if let Some(key) = self.idempotency_key.as_deref() {
            if key.trim().is_empty() || key.len() > 128 || !is_valid_token(key) {
                return Err(CommandRequestValidationError::InvalidIdempotencyKey);
            }
        }
        if self.payload.len() > max_payload_bytes {
            return Err(CommandRequestValidationError::PayloadTooLarge);
        }
        Ok(())
    }

    /// Borrows the UTF-8 payload as bytes.
    pub fn payload_bytes(&self) -> &[u8] {
        self.payload.as_bytes()
    }

    /// Validates and converts this request to the core command envelope.
    pub fn to_envelope(
        &self,
        app_id: AppId,
        node_id: NodeId,
        issued_at_ms: u64,
        max_payload_bytes: usize,
    ) -> RuntimeResult<CommandEnvelope> {
        if let Err(error) = self.validate(max_payload_bytes) {
            return Err(validation_error_to_runtime_error(error));
        }
        self.to_envelope_unchecked(app_id, node_id, issued_at_ms)
    }

    fn to_envelope_unchecked(
        &self,
        app_id: AppId,
        node_id: NodeId,
        issued_at_ms: u64,
    ) -> RuntimeResult<CommandEnvelope> {
        CommandEnvelope::new(
            CommandName::new(self.command_name.clone())?,
            self.command_id.clone(),
            app_id,
            node_id,
            issued_at_ms,
            self.idempotency_key.clone(),
            self.payload_bytes().to_vec(),
        )
    }
}

impl CommandResponse {
    /// Creates an accepted response containing emitted event identities.
    pub fn accepted(events: Vec<CommandResponseEvent>) -> Self {
        Self {
            accepted: true,
            message: None,
            events,
        }
    }

    /// Creates a controlled rejected response.
    pub fn rejected(message: impl Into<String>) -> Self {
        Self {
            accepted: false,
            message: Some(message.into()),
            events: Vec::new(),
        }
    }
}

fn is_valid_token(value: &str) -> bool {
    value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-'))
}

fn requires_idempotency_key(command_name: &str) -> bool {
    command_name != "runtime.ping"
}

fn validation_error_to_runtime_error(
    error: CommandRequestValidationError,
) -> appcore_core::RuntimeError {
    use CommandRequestValidationError as ValidationError;
    let reason = match error {
        ValidationError::EmptyCommandName => "empty_command_name",
        ValidationError::EmptyCommandId => "empty_command_id",
        ValidationError::PayloadTooLarge => "payload_too_large",
        ValidationError::MissingIdempotencyKey => "missing_idempotency_key",
        ValidationError::InvalidIdempotencyKey => "invalid_idempotency_key",
        ValidationError::InvalidCommandName => "invalid_command_name",
        ValidationError::InvalidCommandId => "invalid_command_id",
    };
    appcore_core::RuntimeError::InvalidRequest {
        kind: "command",
        reason,
    }
}

#[cfg(test)]
#[path = "command_contract_tests.rs"]
mod tests;
