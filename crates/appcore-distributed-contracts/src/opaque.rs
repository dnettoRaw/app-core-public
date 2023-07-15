// =============================================================================
//        #######
//     ###       ###     F: opaque.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/02 00:04:12 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:48:56 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Opaque content-envelope transport contracts.

use appcore_types::{CapabilityName, InstanceId, TenantId};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::fmt::{Debug, Formatter};

/// Schema label for the first opaque content-envelope transport contract.
pub const OPAQUE_CONTENT_ENVELOPE_SCHEMA_V1: &str = "appcore.opaque-content.v1";

/// Gateway/sync transport metadata for an opaque encrypted content envelope.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpaqueContentEnvelopeV1 {
    /// Stable schema label.
    pub schema: String,
    /// Tenant routing boundary.
    pub tenant_id: TenantId,
    /// Runtime instance that produced this envelope.
    pub sender_instance_id: InstanceId,
    /// Idempotency and deduplication identifier.
    pub message_id: String,
    /// Optional correlation identifier.
    pub correlation_id: Option<String>,
    /// Capability required to consume this envelope.
    pub capability: CapabilityName,
    /// Opaque encrypted content envelope, usually DNT bytes.
    pub content_envelope: Vec<u8>,
    /// Content-envelope format version, such as DNT envelope version.
    pub envelope_version: u16,
    /// Creation timestamp in Unix milliseconds.
    pub created_at_ms: u64,
    /// Expiration timestamp in Unix milliseconds.
    pub expires_at_ms: u64,
    /// Optional transport priority. Lower values are higher priority.
    pub priority: Option<u8>,
}

impl Debug for OpaqueContentEnvelopeV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpaqueContentEnvelopeV1")
            .field("schema", &self.schema)
            .field("tenant_id", &self.tenant_id)
            .field("sender_instance_id", &self.sender_instance_id)
            .field("message_id", &self.message_id)
            .field("correlation_id", &self.correlation_id)
            .field("capability", &self.capability)
            .field("content_envelope_bytes", &self.content_envelope.len())
            .field("envelope_version", &self.envelope_version)
            .field("created_at_ms", &self.created_at_ms)
            .field("expires_at_ms", &self.expires_at_ms)
            .field("priority", &self.priority)
            .finish()
    }
}

impl OpaqueContentEnvelopeV1 {
    /// Creates a versioned opaque content envelope.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tenant_id: TenantId,
        sender_instance_id: InstanceId,
        message_id: impl Into<String>,
        correlation_id: Option<String>,
        capability: CapabilityName,
        content_envelope: Vec<u8>,
        envelope_version: u16,
        created_at_ms: u64,
        expires_at_ms: u64,
        priority: Option<u8>,
    ) -> Self {
        Self {
            schema: OPAQUE_CONTENT_ENVELOPE_SCHEMA_V1.to_string(),
            tenant_id,
            sender_instance_id,
            message_id: message_id.into(),
            correlation_id,
            capability,
            content_envelope,
            envelope_version,
            created_at_ms,
            expires_at_ms,
            priority,
        }
    }

    /// Validates transport metadata without opening the opaque payload.
    pub fn validate_transport(
        &self,
        policy: &OpaqueEnvelopePolicy,
        now_ms: u64,
    ) -> OpaqueEnvelopeDecision {
        if self.schema != OPAQUE_CONTENT_ENVELOPE_SCHEMA_V1 {
            return OpaqueEnvelopeDecision::UnsupportedSchema;
        }
        if self.message_id.trim().is_empty() || self.content_envelope.is_empty() {
            return OpaqueEnvelopeDecision::InvalidEnvelope;
        }
        if self.created_at_ms >= self.expires_at_ms {
            return OpaqueEnvelopeDecision::Expired;
        }
        if self.expires_at_ms <= now_ms {
            return OpaqueEnvelopeDecision::Expired;
        }
        if !policy
            .accepted_envelope_versions
            .contains(&self.envelope_version)
        {
            return OpaqueEnvelopeDecision::UnsupportedEnvelopeVersion;
        }
        if self.content_envelope.len() as u64 > policy.max_payload_bytes {
            return OpaqueEnvelopeDecision::PayloadTooLarge;
        }
        if !policy.accepted_capabilities.contains(&self.capability) {
            return OpaqueEnvelopeDecision::UnsupportedCapability;
        }
        OpaqueEnvelopeDecision::Accepted
    }
}

/// Transport policy for opaque content envelopes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpaqueEnvelopePolicy {
    /// Accepted opaque content-envelope versions.
    pub accepted_envelope_versions: Vec<u16>,
    /// Maximum opaque payload bytes.
    pub max_payload_bytes: u64,
    /// Capabilities this consumer can process.
    pub accepted_capabilities: Vec<CapabilityName>,
}

/// Result of opaque transport validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpaqueEnvelopeDecision {
    /// The envelope may be routed or consumed.
    Accepted,
    /// The transport schema is unknown.
    UnsupportedSchema,
    /// The opaque content-envelope version is not accepted.
    UnsupportedEnvelopeVersion,
    /// The envelope is expired.
    Expired,
    /// The opaque bytes exceed policy.
    PayloadTooLarge,
    /// No compatible consumer capability is available.
    UnsupportedCapability,
    /// The message ID was already observed.
    Duplicate,
    /// Required transport metadata is malformed.
    InvalidEnvelope,
}

/// Bounded in-memory deduplicator keyed by opaque message ID.
#[derive(Debug, Clone)]
pub struct OpaqueEnvelopeDeduplicator {
    max_entries: usize,
    seen: HashSet<String>,
    order: VecDeque<String>,
}

impl OpaqueEnvelopeDeduplicator {
    /// Creates a bounded deduplicator.
    pub fn new(max_entries: usize) -> Self {
        Self {
            max_entries: max_entries.max(1),
            seen: HashSet::new(),
            order: VecDeque::new(),
        }
    }

    /// Records a message ID and reports whether it was new.
    pub fn accept(&mut self, message_id: &str) -> OpaqueEnvelopeDecision {
        if self.seen.contains(message_id) {
            return OpaqueEnvelopeDecision::Duplicate;
        }
        self.seen.insert(message_id.to_string());
        self.order.push_back(message_id.to_string());
        while self.seen.len() > self.max_entries {
            if let Some(oldest) = self.order.pop_front() {
                self.seen.remove(&oldest);
            }
        }
        OpaqueEnvelopeDecision::Accepted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(capability: &str, version: u16) -> OpaqueContentEnvelopeV1 {
        OpaqueContentEnvelopeV1::new(
            TenantId::new("tenant-a").unwrap(),
            InstanceId::new("instance-a").unwrap(),
            "message-a",
            Some("corr-a".to_string()),
            CapabilityName::new(capability).unwrap(),
            vec![1, 2, 3],
            version,
            100,
            200,
            Some(5),
        )
    }

    fn policy() -> OpaqueEnvelopePolicy {
        OpaqueEnvelopePolicy {
            accepted_envelope_versions: vec![1],
            max_payload_bytes: 8,
            accepted_capabilities: vec![CapabilityName::new("sync.consumer").unwrap()],
        }
    }

    #[test]
    fn opaque_envelope_validates_transport_only() {
        assert_eq!(
            envelope("sync.consumer", 1).validate_transport(&policy(), 150),
            OpaqueEnvelopeDecision::Accepted
        );
    }

    #[test]
    fn opaque_envelope_rejects_old_consumer_version_and_capability() {
        assert_eq!(
            envelope("sync.consumer", 2).validate_transport(&policy(), 150),
            OpaqueEnvelopeDecision::UnsupportedEnvelopeVersion
        );
        assert_eq!(
            envelope("sync.publisher", 1).validate_transport(&policy(), 150),
            OpaqueEnvelopeDecision::UnsupportedCapability
        );
    }

    #[test]
    fn opaque_envelope_deduplicates_message_id() {
        let mut dedupe = OpaqueEnvelopeDeduplicator::new(16);
        assert_eq!(dedupe.accept("message-a"), OpaqueEnvelopeDecision::Accepted);
        assert_eq!(
            dedupe.accept("message-a"),
            OpaqueEnvelopeDecision::Duplicate
        );
    }

    #[test]
    fn opaque_envelope_rejects_malformed_transport_metadata() {
        let mut malformed = envelope("sync.consumer", 1);
        malformed.message_id.clear();
        assert_eq!(
            malformed.validate_transport(&policy(), 150),
            OpaqueEnvelopeDecision::InvalidEnvelope
        );

        let mut expired_before_creation = envelope("sync.consumer", 1);
        expired_before_creation.expires_at_ms = expired_before_creation.created_at_ms;
        assert_eq!(
            expired_before_creation.validate_transport(&policy(), 150),
            OpaqueEnvelopeDecision::Expired
        );
    }

    #[test]
    fn opaque_envelope_debug_never_prints_content_bytes() {
        let marker = b"secret-marker-must-not-appear";
        let mut envelope = envelope("sync.consumer", 1);
        envelope.content_envelope = marker.to_vec();

        let output = format!("{envelope:?}");
        assert!(!output.contains(std::str::from_utf8(marker).unwrap()));
        assert!(output.contains("content_envelope_bytes"));
    }
}
