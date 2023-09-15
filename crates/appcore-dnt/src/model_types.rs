// =============================================================================
//        #######
//     ###       ###     F: model_types.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/20 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/20 12:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Public DNT context, operation options, and authenticated results.

use crate::{CodecId, ContentType, DntHeader, KeyId};
use appcore_contracts::ApplicationId;
use appcore_types::TenantId;
use zeroize::Zeroize;

/// DNT context bound to key resolution and authenticated header validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DntContext {
    /// Application that owns the envelope.
    pub application_id: ApplicationId,
    /// Optional tenant boundary.
    pub tenant_id: Option<TenantId>,
    /// Logical content type.
    pub content_type: ContentType,
    /// Payload codec identifier.
    pub codec_id: CodecId,
    /// Payload schema version.
    pub schema_version: u32,
}

impl DntContext {
    /// Creates context from an inspected header.
    pub fn from_header(header: &DntHeader) -> Self {
        Self {
            application_id: header.application_id.clone(),
            tenant_id: header.tenant_id.clone(),
            content_type: header.content_type.clone(),
            codec_id: header.codec_id.clone(),
            schema_version: header.schema_version,
        }
    }
}

/// Options used when sealing a payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DntSealOptions {
    /// Application that owns the envelope.
    pub application_id: ApplicationId,
    /// Optional tenant boundary.
    pub tenant_id: Option<TenantId>,
    /// Logical content type.
    pub content_type: ContentType,
    /// Payload schema version.
    pub schema_version: u32,
    /// Rotation-aware key identifier.
    pub key_id: KeyId,
    /// Creation timestamp in Unix milliseconds.
    pub created_at_ms: u64,
    /// Authenticated public metadata.
    pub public_metadata: Vec<u8>,
    /// Metadata stored inside the encrypted plaintext.
    pub encrypted_metadata: Vec<u8>,
    /// Reserved flags for future envelope behavior.
    pub flags: u32,
    /// Optional maximum encoded payload size.
    pub max_payload_bytes: Option<u64>,
}

/// Options used when opening or verifying an envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DntOpenOptions {
    /// Required application identity.
    pub application_id: ApplicationId,
    /// Required tenant identity. `None` accepts only tenantless envelopes.
    pub tenant_id: Option<TenantId>,
    /// Required logical content type.
    pub content_type: ContentType,
    /// Optional maximum encoded payload size.
    pub max_payload_bytes: Option<u64>,
}

/// Authenticated and decoded DNT content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenedDnt {
    /// Authenticated header.
    pub header: DntHeader,
    /// Decoded payload bytes.
    pub payload: Vec<u8>,
    /// Authenticated encrypted metadata.
    pub encrypted_metadata: Vec<u8>,
}

impl OpenedDnt {
    /// Zeroizes the returned plaintext payload and encrypted metadata buffers.
    pub fn zeroize_plaintext(&mut self) {
        self.payload.zeroize();
        self.encrypted_metadata.zeroize();
    }
}

/// Result of cryptographic verification without returning plaintext.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedDnt {
    /// Authenticated header.
    pub header: DntHeader,
}
