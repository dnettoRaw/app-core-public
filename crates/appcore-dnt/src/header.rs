// =============================================================================
//        #######
//     ###       ###     F: header.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/02 00:04:12 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:07:11 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Canonical DNT header encoding.

use crate::{CodecId, ContentType, DntCompression, DntError, DntResult, KeyId};
use appcore_contracts::ApplicationId;
use appcore_types::TenantId;

/// DNT magic bytes. File extensions are conventions only.
pub const DNT_MAGIC: [u8; 8] = *b"APDNT\0\0\x01";
/// Current DNT envelope version.
pub const DNT_ENVELOPE_VERSION_V1: u16 = 1;
/// Maximum accepted header size.
pub const DNT_MAX_HEADER_BYTES: usize = 64 * 1024;
/// Maximum encrypted metadata accepted by the in-memory V1 envelope.
pub const DNT_MAX_ENCRYPTED_METADATA_BYTES: usize = 64 * 1024;
/// Conventional JSON content type.
pub const DNT_CONTENT_JSON: &str = "application/json";
/// Conventional arbitrary bytes content type.
pub const DNT_CONTENT_OCTET_STREAM: &str = "application/octet-stream";
/// AppCore secret material content type.
pub const DNT_CONTENT_SECRET: &str = "appcore/secret";
/// AppCore snapshot content type.
pub const DNT_CONTENT_SNAPSHOT: &str = "appcore/snapshot";
/// AppCore sync event content type.
pub const DNT_CONTENT_SYNC_EVENT: &str = "appcore/sync-event";
/// AppCore backup content type.
pub const DNT_CONTENT_BACKUP: &str = "appcore/backup";

const MIN_PREFIX_BYTES: usize = 14;
const NONCE_BYTES: usize = 24;
const HASH_BYTES: usize = 32;

/// Authenticated encryption algorithm used by this envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DntAlgorithm {
    /// XChaCha20-Poly1305 with a 256-bit key and 192-bit nonce.
    XChaCha20Poly1305,
}

impl DntAlgorithm {
    pub(crate) fn id(self) -> u16 {
        match self {
            Self::XChaCha20Poly1305 => 1,
        }
    }

    fn from_id(value: u16) -> DntResult<Self> {
        match value {
            1 => Ok(Self::XChaCha20Poly1305),
            _ => Err(DntError::UnsupportedVersion),
        }
    }
}

/// Structurally parsed DNT header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DntHeader {
    /// Envelope version.
    pub envelope_version: u16,
    /// Canonical header length in bytes.
    pub header_length: u32,
    /// Extension flags reserved for future formats.
    pub flags: u32,
    /// Authenticated encryption algorithm.
    pub algorithm: DntAlgorithm,
    /// Application that owns this envelope.
    pub application_id: ApplicationId,
    /// Optional tenant boundary.
    pub tenant_id: Option<TenantId>,
    /// Logical content type.
    pub content_type: ContentType,
    /// Payload codec identifier.
    pub codec_id: CodecId,
    /// Rotation-aware key identifier.
    pub key_id: KeyId,
    /// Payload schema version, independent of envelope version.
    pub schema_version: u32,
    /// Creation timestamp in Unix milliseconds.
    pub created_at_ms: u64,
    /// Stored encoded payload length, after any authenticated compression.
    pub payload_length: u64,
    /// AEAD nonce.
    pub nonce: [u8; NONCE_BYTES],
    /// Keyed digest of the stored encoded payload.
    pub payload_hash: [u8; HASH_BYTES],
    /// Authenticated public metadata bytes.
    pub public_metadata: Vec<u8>,
    /// Encrypted metadata length stored inside AEAD plaintext.
    pub encrypted_metadata_length: u32,
}

impl DntHeader {
    /// Returns the nonce bytes.
    pub fn nonce(&self) -> &[u8; NONCE_BYTES] {
        &self.nonce
    }

    /// Returns the authenticated payload compression mode.
    pub fn compression(&self) -> DntCompression {
        if self.flags & crate::DNT_FLAG_PAYLOAD_DEFLATE != 0 {
            return DntCompression::Deflate;
        }
        DntCompression::None
    }
}

pub(crate) struct HeaderParts {
    pub(crate) flags: u32,
    pub(crate) algorithm: DntAlgorithm,
    pub(crate) application_id: ApplicationId,
    pub(crate) tenant_id: Option<TenantId>,
    pub(crate) content_type: ContentType,
    pub(crate) codec_id: CodecId,
    pub(crate) key_id: KeyId,
    pub(crate) schema_version: u32,
    pub(crate) created_at_ms: u64,
    pub(crate) payload_length: u64,
    pub(crate) nonce: [u8; NONCE_BYTES],
    pub(crate) payload_hash: [u8; HASH_BYTES],
    pub(crate) public_metadata: Vec<u8>,
    pub(crate) encrypted_metadata_length: u32,
}

pub(crate) fn encode_header(parts: HeaderParts) -> DntResult<Vec<u8>> {
    if parts.public_metadata.len() > DNT_MAX_HEADER_BYTES
        || parts.encrypted_metadata_length as usize > DNT_MAX_ENCRYPTED_METADATA_BYTES
    {
        return Err(DntError::PayloadTooLarge);
    }
    let capacity = 256usize
        .checked_add(parts.public_metadata.len())
        .ok_or(DntError::InvalidFormat)?;
    let mut header = Vec::with_capacity(capacity);
    header.extend_from_slice(&DNT_MAGIC);
    put_u16(&mut header, DNT_ENVELOPE_VERSION_V1);
    put_u32(&mut header, 0);
    put_u32(&mut header, parts.flags);
    put_u16(&mut header, parts.algorithm.id());
    put_u32(&mut header, parts.schema_version);
    put_u64(&mut header, parts.created_at_ms);
    put_u64(&mut header, parts.payload_length);
    header.extend_from_slice(&parts.nonce);
    header.extend_from_slice(&parts.payload_hash);
    put_u32(
        &mut header,
        checked_u32(parts.public_metadata.len(), DntError::InvalidFormat)?,
    );
    put_u32(&mut header, parts.encrypted_metadata_length);
    put_text(&mut header, parts.application_id.as_str())?;
    put_optional_text(
        &mut header,
        parts.tenant_id.as_ref().map(|value| value.as_str()),
    )?;
    put_text(&mut header, parts.content_type.as_str())?;
    put_text(&mut header, parts.codec_id.as_str())?;
    put_text(&mut header, parts.key_id.as_str())?;
    header.extend_from_slice(&parts.public_metadata);
    if header.len() > DNT_MAX_HEADER_BYTES {
        return Err(DntError::InvalidFormat);
    }
    let length = checked_u32(header.len(), DntError::InvalidFormat)?;
    header[10..14].copy_from_slice(&length.to_be_bytes());
    Ok(header)
}

/// Structurally inspects a DNT header without resolving keys or decrypting.
pub fn inspect_header(input: &[u8]) -> DntResult<DntHeader> {
    if input.len() < MIN_PREFIX_BYTES || input[..8] != DNT_MAGIC {
        return Err(DntError::InvalidFormat);
    }
    let version = read_u16(input, 8)?;
    if version != DNT_ENVELOPE_VERSION_V1 {
        return Err(DntError::UnsupportedVersion);
    }
    let header_length = read_u32(input, 10)?;
    let header_len = usize::try_from(header_length).map_err(|_| DntError::InvalidFormat)?;
    if !(MIN_PREFIX_BYTES..=DNT_MAX_HEADER_BYTES).contains(&header_len) || input.len() < header_len
    {
        return Err(DntError::InvalidFormat);
    }
    parse_header(&input[..header_len], header_length)
}

fn parse_header(input: &[u8], header_length: u32) -> DntResult<DntHeader> {
    let mut cursor = 14usize;
    let flags = take_u32(input, &mut cursor)?;
    crate::flags::validate_flags(flags)?;
    let algorithm = DntAlgorithm::from_id(take_u16(input, &mut cursor)?)?;
    let schema_version = take_u32(input, &mut cursor)?;
    let created_at_ms = take_u64(input, &mut cursor)?;
    let payload_length = take_u64(input, &mut cursor)?;
    let nonce = take_array::<NONCE_BYTES>(input, &mut cursor)?;
    let payload_hash = take_array::<HASH_BYTES>(input, &mut cursor)?;
    let public_metadata_length = take_u32(input, &mut cursor)?;
    let encrypted_metadata_length = take_u32(input, &mut cursor)?;
    if encrypted_metadata_length as usize > DNT_MAX_ENCRYPTED_METADATA_BYTES {
        return Err(DntError::PayloadTooLarge);
    }
    let application_id =
        ApplicationId::new(take_text(input, &mut cursor)?).map_err(|_| DntError::InvalidFormat)?;
    let tenant_text = take_optional_text(input, &mut cursor)?;
    let tenant_id = tenant_text
        .map(TenantId::new)
        .transpose()
        .map_err(|_| DntError::InvalidFormat)?;
    let content_type = ContentType::new(take_text(input, &mut cursor)?)?;
    let codec_id = CodecId::new(take_text(input, &mut cursor)?)?;
    let key_id = KeyId::new(take_text(input, &mut cursor)?)?;
    let metadata_len =
        usize::try_from(public_metadata_length).map_err(|_| DntError::InvalidFormat)?;
    let public_metadata = take_bytes(input, &mut cursor, metadata_len)?.to_vec();
    if cursor != input.len() {
        return Err(DntError::InvalidFormat);
    }
    Ok(DntHeader {
        envelope_version: DNT_ENVELOPE_VERSION_V1,
        header_length,
        flags,
        algorithm,
        application_id,
        tenant_id,
        content_type,
        codec_id,
        key_id,
        schema_version,
        created_at_ms,
        payload_length,
        nonce,
        payload_hash,
        public_metadata,
        encrypted_metadata_length,
    })
}

fn put_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn put_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn put_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn put_text(output: &mut Vec<u8>, value: &str) -> DntResult<()> {
    let bytes = value.as_bytes();
    let length = u16::try_from(bytes.len()).map_err(|_| DntError::InvalidFormat)?;
    put_u16(output, length);
    output.extend_from_slice(bytes);
    Ok(())
}

fn put_optional_text(output: &mut Vec<u8>, value: Option<&str>) -> DntResult<()> {
    match value {
        Some(value) => put_text(output, value),
        None => {
            put_u16(output, 0);
            Ok(())
        }
    }
}

fn read_u16(input: &[u8], offset: usize) -> DntResult<u16> {
    let bytes = input
        .get(offset..offset + 2)
        .ok_or(DntError::InvalidFormat)?;
    Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
}

fn read_u32(input: &[u8], offset: usize) -> DntResult<u32> {
    let bytes = input
        .get(offset..offset + 4)
        .ok_or(DntError::InvalidFormat)?;
    Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn take_u16(input: &[u8], cursor: &mut usize) -> DntResult<u16> {
    let value = read_u16(input, *cursor)?;
    *cursor += 2;
    Ok(value)
}

fn take_u32(input: &[u8], cursor: &mut usize) -> DntResult<u32> {
    let value = read_u32(input, *cursor)?;
    *cursor += 4;
    Ok(value)
}

fn take_u64(input: &[u8], cursor: &mut usize) -> DntResult<u64> {
    let bytes = take_bytes(input, cursor, 8)?;
    Ok(u64::from_be_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}

fn take_array<const N: usize>(input: &[u8], cursor: &mut usize) -> DntResult<[u8; N]> {
    let mut output = [0u8; N];
    output.copy_from_slice(take_bytes(input, cursor, N)?);
    Ok(output)
}

fn take_text(input: &[u8], cursor: &mut usize) -> DntResult<String> {
    let length = take_u16(input, cursor)? as usize;
    if length == 0 {
        return Err(DntError::InvalidFormat);
    }
    let bytes = take_bytes(input, cursor, length)?;
    std::str::from_utf8(bytes)
        .map(str::to_string)
        .map_err(|_| DntError::InvalidFormat)
}

fn take_optional_text(input: &[u8], cursor: &mut usize) -> DntResult<Option<String>> {
    let length = take_u16(input, cursor)? as usize;
    if length == 0 {
        return Ok(None);
    }
    let bytes = take_bytes(input, cursor, length)?;
    std::str::from_utf8(bytes)
        .map(|value| Some(value.to_string()))
        .map_err(|_| DntError::InvalidFormat)
}

fn take_bytes<'a>(input: &'a [u8], cursor: &mut usize, length: usize) -> DntResult<&'a [u8]> {
    let end = cursor.checked_add(length).ok_or(DntError::InvalidFormat)?;
    let bytes = input.get(*cursor..end).ok_or(DntError::InvalidFormat)?;
    *cursor = end;
    Ok(bytes)
}

fn checked_u32(value: usize, error: DntError) -> DntResult<u32> {
    u32::try_from(value).map_err(|_| error)
}
