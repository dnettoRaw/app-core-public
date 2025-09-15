// =============================================================================
//        #######
//     ###       ###     F: query_contract.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/01 13:57:57 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 13:45:20 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Shared query request/response API contract for transports.

use serde::{Deserialize, Serialize};

/// Version 1 side-effect-free query request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryRequest {
    /// Declared application or Runtime query capability.
    pub query_name: String,
    /// Caller-assigned request identity.
    pub query_id: String,
    /// Structured application-owned query payload.
    pub payload: serde_json::Value,
}

/// Version 1 controlled query response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryResponse {
    /// Whether query execution succeeded.
    pub ok: bool,
    /// Controlled rejection detail, when present.
    pub message: Option<String>,
    /// Structured application-owned response payload.
    pub payload: serde_json::Value,
}

/// Validation failures defined by the query V1 contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryRequestValidationError {
    /// The query name is empty.
    EmptyQueryName,
    /// The query identifier is empty.
    EmptyQueryId,
    /// The query name is malformed.
    InvalidQueryName,
    /// The query identifier is malformed.
    InvalidQueryId,
    /// The serialized payload exceeds the configured request bound.
    PayloadTooLarge,
}

impl QueryRequest {
    /// Validates identifiers and the serialized payload bound.
    pub fn validate(&self, max_payload_bytes: usize) -> Result<(), QueryRequestValidationError> {
        if self.query_name.trim().is_empty() {
            return Err(QueryRequestValidationError::EmptyQueryName);
        }
        if self.query_id.trim().is_empty() {
            return Err(QueryRequestValidationError::EmptyQueryId);
        }
        if self.query_name.len() > 128 || !is_valid_token(&self.query_name) {
            return Err(QueryRequestValidationError::InvalidQueryName);
        }
        if self.query_id.len() > 128 || !is_valid_token(&self.query_id) {
            return Err(QueryRequestValidationError::InvalidQueryId);
        }
        if self.payload_bytes().len() > max_payload_bytes {
            return Err(QueryRequestValidationError::PayloadTooLarge);
        }
        Ok(())
    }

    /// Serializes the structured payload to JSON bytes.
    pub fn payload_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(&self.payload).unwrap_or_default()
    }
}

impl QueryResponse {
    /// Creates a successful response with a structured payload.
    pub fn ok(payload: serde_json::Value) -> Self {
        Self {
            ok: true,
            message: None,
            payload,
        }
    }

    /// Creates a controlled rejected response.
    pub fn rejected(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            message: Some(message.into()),
            payload: serde_json::Value::Object(serde_json::Map::new()),
        }
    }
}

fn is_valid_token(value: &str) -> bool {
    value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-'))
}

#[cfg(test)]
#[path = "query_contract_tests.rs"]
mod tests;
