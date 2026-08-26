// =============================================================================
//        #######
//     ###       ###     F: validation.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::*;

/// Validation limits and local identity expected by the peer RPC host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerRpcValidationConfig {
    /// Tenant accepted by this host.
    pub local_tenant_id: TenantId,
    /// Cluster accepted by this host.
    pub local_cluster_id: ClusterId,
    /// Core identity targeted by incoming envelopes.
    pub local_core_id: CoreId,
    /// Maximum decoded application payload size.
    pub max_payload_bytes: usize,
    /// Maximum tolerated clock skew and replay window.
    pub nonce_window_ms: u64,
}

/// Stateful validator for peer identity, protocol, expiry, integrity, and replay.
#[derive(Debug, Clone)]
pub struct PeerRpcValidator {
    config: PeerRpcValidationConfig,
    local_protocol_version: ProtocolVersion,
    nonce_store: Arc<dyn crate::PeerNonceStore>,
}
impl PeerRpcValidator {
    /// Creates a validator using the default protocol version.
    pub fn new(config: PeerRpcValidationConfig) -> Self {
        Self {
            config,
            local_protocol_version: ProtocolVersion::default(),
            nonce_store: Arc::new(crate::InMemoryPeerNonceStore::default()),
        }
    }

    /// Sets the protocol version accepted by this host.
    pub fn with_protocol_version(mut self, protocol_version: ProtocolVersion) -> Self {
        self.local_protocol_version = protocol_version;
        self
    }

    /// Replaces process-local replay tracking with a deployment-selected store.
    pub fn with_nonce_store(mut self, nonce_store: Arc<dyn crate::PeerNonceStore>) -> Self {
        self.nonce_store = nonce_store;
        self
    }

    pub(crate) fn max_envelope_bytes(&self) -> usize {
        self.config
            .max_payload_bytes
            .saturating_mul(4)
            .saturating_add(MAX_ENVELOPE_OVERHEAD_BYTES)
    }

    /// Validates an envelope and records its nonce to prevent replay.
    pub fn validate(&self, envelope: &PeerRpcEnvelope, now_ms: u64) -> Result<(), PeerRpcError> {
        validate_envelope_identifiers(envelope)?;
        if envelope.payload.len() > self.config.max_payload_bytes {
            return Err(PeerRpcError::PayloadTooLarge);
        }
        if envelope.tenant_id != self.config.local_tenant_id {
            return Err(PeerRpcError::TenantMismatch);
        }
        if envelope.cluster_id != self.config.local_cluster_id {
            return Err(PeerRpcError::ClusterMismatch);
        }
        if envelope.target_core_id != self.config.local_core_id {
            return Err(PeerRpcError::TargetMismatch);
        }
        if !self
            .local_protocol_version
            .is_compatible_with(envelope.protocol_version)
        {
            return Err(PeerRpcError::ProtocolMismatch);
        }
        let window_ms = self.config.nonce_window_ms.max(1);
        if envelope.timestamp_ms >= envelope.expires_at_ms
            || envelope.expires_at_ms <= now_ms
            || envelope.timestamp_ms > now_ms.saturating_add(window_ms)
            || now_ms > envelope.timestamp_ms.saturating_add(window_ms)
        {
            return Err(PeerRpcError::Expired);
        }
        if envelope.body_hash != payload_hash(&envelope.payload) {
            return Err(PeerRpcError::InvalidBodyHash);
        }
        if let Some(trace) = &envelope.trace {
            if trace.trace_id != envelope.trace_id
                || trace.tenant_id != envelope.tenant_id
                || trace.current_core_id != envelope.source_core_id
            {
                return Err(PeerRpcError::InvalidEnvelope(
                    "trace_context_mismatch".to_string(),
                ));
            }
        }
        let nonce_expires_at_ms = envelope.expires_at_ms.min(now_ms.saturating_add(window_ms));
        self.nonce_store
            .check_and_record(&envelope.nonce, nonce_expires_at_ms, now_ms)?;
        Ok(())
    }

    /// Validates V2 open identity, isolation, deadline, trace, and nonce replay.
    pub fn validate_stream_open_v2(
        &self,
        open: &crate::v2::PeerRpcStreamOpenV2,
        now_ms: u64,
    ) -> Result<(), PeerRpcError> {
        validate_stream_open_identifiers(open)?;
        if open.protocol_version.as_u16() != crate::v2::PEER_RPC_PROTOCOL_VERSION_V2 {
            return Err(PeerRpcError::ProtocolMismatch);
        }
        if open.direction != crate::v2::PeerRpcStreamDirectionV2::Request {
            return Err(PeerRpcError::InvalidEnvelope(
                "stream_direction_mismatch".to_string(),
            ));
        }
        if open.tenant_id != self.config.local_tenant_id {
            return Err(PeerRpcError::TenantMismatch);
        }
        if open.cluster_id != self.config.local_cluster_id {
            return Err(PeerRpcError::ClusterMismatch);
        }
        if open.target_core_id != self.config.local_core_id {
            return Err(PeerRpcError::TargetMismatch);
        }
        let window_ms = self.config.nonce_window_ms.max(1);
        if open.timestamp_ms >= open.deadline_ms
            || open.deadline_ms <= now_ms
            || open.timestamp_ms > now_ms.saturating_add(window_ms)
            || now_ms > open.timestamp_ms.saturating_add(window_ms)
        {
            return Err(PeerRpcError::Expired);
        }
        if open.call_kind == PeerRpcCallKind::Command && open.idempotency_key.is_none() {
            return Err(PeerRpcError::InvalidEnvelope(
                "stream_command_idempotency_required".to_string(),
            ));
        }
        if let Some(trace) = &open.trace {
            if trace.trace_id != open.trace_id
                || trace.tenant_id != open.tenant_id
                || trace.current_core_id != open.source_core_id
            {
                return Err(PeerRpcError::InvalidEnvelope(
                    "trace_context_mismatch".to_string(),
                ));
            }
        }
        let nonce_expires_at_ms = open.deadline_ms.min(now_ms.saturating_add(window_ms));
        self.nonce_store
            .check_and_record(&open.nonce, nonce_expires_at_ms, now_ms)
    }
}

fn validate_stream_open_identifiers(
    open: &crate::v2::PeerRpcStreamOpenV2,
) -> Result<(), PeerRpcError> {
    for (kind, value) in [
        ("PeerRequestId", open.request_id.as_str()),
        ("PeerStreamId", open.stream_id.as_str()),
        ("TraceId", open.trace_id.as_str()),
        ("PeerNonce", open.nonce.as_str()),
    ] {
        validate_identifier(kind, value)
            .map_err(|_| PeerRpcError::InvalidEnvelope("invalid_identifier".to_string()))?;
    }
    if let Some(idempotency_key) = &open.idempotency_key {
        validate_identifier("IdempotencyKey", idempotency_key)
            .map_err(|_| PeerRpcError::InvalidEnvelope("invalid_idempotency_key".to_string()))?;
    }
    open.source_core_id
        .validate()
        .and_then(|_| open.target_core_id.validate())
        .and_then(|_| open.tenant_id.validate())
        .and_then(|_| open.cluster_id.validate())
        .and_then(|_| open.capability.validate())
        .map_err(|_| PeerRpcError::InvalidEnvelope("invalid_identifier".to_string()))
}

fn validate_envelope_identifiers(envelope: &PeerRpcEnvelope) -> Result<(), PeerRpcError> {
    for (kind, value) in [
        ("PeerRequestId", envelope.request_id.as_str()),
        ("TraceId", envelope.trace_id.as_str()),
        ("PeerNonce", envelope.nonce.as_str()),
    ] {
        validate_identifier(kind, value)
            .map_err(|_| PeerRpcError::InvalidEnvelope("invalid_identifier".to_string()))?;
    }
    if let Some(idempotency_key) = &envelope.idempotency_key {
        validate_identifier("IdempotencyKey", idempotency_key)
            .map_err(|_| PeerRpcError::InvalidEnvelope("invalid_idempotency_key".to_string()))?;
    }
    envelope
        .source_core_id
        .validate()
        .and_then(|_| envelope.target_core_id.validate())
        .and_then(|_| envelope.tenant_id.validate())
        .and_then(|_| envelope.cluster_id.validate())
        .and_then(|_| envelope.capability.validate())
        .map_err(|_| PeerRpcError::InvalidEnvelope("invalid_identifier".to_string()))
}

/// Returns the hexadecimal SHA-256 digest of an application payload.
pub fn payload_hash(payload: &[u8]) -> String {
    let digest = Sha256::digest(payload);
    hex_encode(&digest)
}

/// Hash signed by peer bearer tokens. It binds routing metadata and payload integrity.
pub fn envelope_signing_hash(envelope: &PeerRpcEnvelope) -> String {
    let mut hasher = Sha256::new();
    hash_field(&mut hasher, envelope.request_id.as_bytes());
    hash_field(&mut hasher, envelope.trace_id.as_bytes());
    hasher.update(envelope.protocol_version.as_u16().to_be_bytes());
    hash_field(&mut hasher, envelope.source_core_id.as_str().as_bytes());
    hash_field(&mut hasher, envelope.target_core_id.as_str().as_bytes());
    hash_field(&mut hasher, envelope.tenant_id.as_str().as_bytes());
    hash_field(&mut hasher, envelope.cluster_id.as_str().as_bytes());
    hasher.update(envelope.timestamp_ms.to_be_bytes());
    hasher.update(envelope.expires_at_ms.to_be_bytes());
    hash_field(&mut hasher, envelope.nonce.as_bytes());
    hash_field(&mut hasher, envelope.capability.as_str().as_bytes());
    hash_field(&mut hasher, envelope.body_hash.as_bytes());
    hash_optional_field(&mut hasher, envelope.idempotency_key.as_deref());
    if let Some(trace) = &envelope.trace {
        hasher.update([1]);
        hash_field(&mut hasher, trace.trace_id.as_bytes());
        hash_field(&mut hasher, trace.span_id.as_bytes());
        hash_optional_field(&mut hasher, trace.parent_span_id.as_deref());
        hash_field(&mut hasher, trace.originating_core_id.as_str().as_bytes());
        hash_field(&mut hasher, trace.current_core_id.as_str().as_bytes());
        hash_field(&mut hasher, trace.tenant_id.as_str().as_bytes());
        hash_optional_field(&mut hasher, trace.command_id.as_deref());
    } else {
        hasher.update([0]);
    }
    hex_encode(&hasher.finalize())
}

fn hash_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn hash_optional_field(hasher: &mut Sha256, value: Option<&str>) {
    if let Some(value) = value {
        hasher.update([1]);
        hash_field(hasher, value.as_bytes());
    } else {
        hasher.update([0]);
    }
}

/// Returns the stable query endpoint path.
pub fn route_for_query() -> &'static str {
    PEER_QUERY_PATH
}

/// Returns the stable command endpoint path.
pub fn route_for_command() -> &'static str {
    PEER_COMMAND_PATH
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}
