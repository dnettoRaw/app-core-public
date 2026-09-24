// =============================================================================
//        #######
//     ###       ###     F: peer.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/24 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/24 00:00:00 by dnettoRaw
//      ###########      S: 1.0.3-rc
// =============================================================================

//! Update payloads carried inside authenticated Peer RPC V2 streams.

use crate::{ArtifactTarget, UpdateError, UpdateResult};
use appcore_contracts::{ApplicationId, BuildId};
use serde::{Deserialize, Serialize};

/// Capability used to negotiate availability of one immutable artifact.
pub const UPDATE_OFFER_CAPABILITY: &str = "appcore.update.offer";
/// Capability used to request one bounded artifact byte range.
pub const UPDATE_CHUNK_CAPABILITY: &str = "appcore.update.chunk";
/// Maximum serialized metadata accepted by the update payload contracts.
pub const UPDATE_PEER_METADATA_MAX_BYTES: usize = 16 * 1024;

/// Typed status returned by an update peer before byte streaming begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactPeerStatusV2 {
    /// The peer can serve the requested artifact.
    Accepted,
    /// Transport or Gateway authorization rejected the request.
    Unauthorized,
    /// The peer is temporarily unable to serve the artifact.
    Unavailable,
    /// The requested catalog state is no longer current.
    Stale,
    /// Identity, protocol or target is incompatible.
    Incompatible,
    /// The peer found invalid or corrupted artifact state.
    Corrupted,
}

/// Path-free offer metadata sent through a Peer RPC V2 request stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactOfferRequestV2 {
    /// Application identity selected by the caller.
    pub application_id: ApplicationId,
    /// Immutable build identity selected by the caller.
    pub build_id: BuildId,
    /// Platform target selected by the caller.
    pub target: ArtifactTarget,
    /// Artifact digest expected by the caller.
    pub sha256: String,
    /// Exact artifact size expected by the caller.
    pub size_bytes: u64,
    /// Application/runtime protocol expected by the caller.
    pub protocol_version: String,
}

/// Response metadata for an artifact offer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactOfferResponseV2 {
    /// Peer decision before opening a byte stream.
    pub status: ArtifactPeerStatusV2,
    /// Digest repeated by the peer.
    pub sha256: String,
    /// Size repeated by the peer.
    pub size_bytes: u64,
    /// Maximum decoded chunk accepted by the peer.
    pub max_chunk_bytes: u32,
    /// Application/runtime protocol repeated by the peer.
    pub protocol_version: String,
}

/// Path-free bounded byte-range request carried by a Peer RPC V2 stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactChunkRequestV2 {
    /// Artifact digest addressed by this request.
    pub sha256: String,
    /// Zero-based byte offset.
    pub offset: u64,
    /// Requested decoded byte length.
    pub len: u32,
}

/// Metadata repeated beside the decoded bytes in a Peer RPC V2 response stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactChunkResponseV2 {
    /// Artifact digest repeated by the peer.
    pub sha256: String,
    /// Offset repeated by the peer.
    pub offset: u64,
    /// Decoded byte length repeated by the peer.
    pub len: u32,
    /// SHA-256 of the decoded response chunk.
    pub chunk_sha256: String,
    /// Complete artifact size repeated by the peer.
    pub total_size_bytes: u64,
}

impl ArtifactOfferRequestV2 {
    /// Validates bounded offer metadata before it enters Peer RPC.
    pub fn validate(&self) -> UpdateResult<()> {
        validate_digest(&self.sha256)?;
        validate_protocol(&self.protocol_version)?;
        if self.size_bytes == 0 {
            return Err(UpdateError::InvalidArtifact(
                "peer offer size_bytes must be greater than zero".to_string(),
            ));
        }
        validate_metadata_size(self)
    }
}

impl ArtifactOfferResponseV2 {
    /// Validates that an offer response repeats the caller's identity exactly.
    pub fn validate_against(&self, request: &ArtifactOfferRequestV2) -> UpdateResult<()> {
        validate_digest(&self.sha256)?;
        validate_protocol(&self.protocol_version)?;
        if self.status == ArtifactPeerStatusV2::Accepted
            && (self.sha256 != request.sha256
                || self.size_bytes != request.size_bytes
                || self.protocol_version != request.protocol_version
                || self.max_chunk_bytes == 0)
        {
            return Err(UpdateError::Transfer(
                "peer offer response does not repeat the requested artifact identity".to_string(),
            ));
        }
        validate_metadata_size(self)
    }
}

impl ArtifactChunkRequestV2 {
    /// Validates a range request against the offered artifact and chunk limit.
    pub fn validate_against(
        &self,
        offer: &ArtifactOfferRequestV2,
        max_chunk_bytes: u32,
    ) -> UpdateResult<()> {
        validate_digest(&self.sha256)?;
        if self.sha256 != offer.sha256
            || self.len == 0
            || self.len > max_chunk_bytes
            || self.offset >= offer.size_bytes
            || self.offset.saturating_add(self.len as u64) > offer.size_bytes
        {
            return Err(UpdateError::Transfer(
                "peer chunk request is outside the offered artifact bounds".to_string(),
            ));
        }
        validate_metadata_size(self)
    }
}

impl ArtifactChunkResponseV2 {
    /// Validates response metadata before accepting the decoded stream bytes.
    pub fn validate_against(
        &self,
        request: &ArtifactChunkRequestV2,
        total_size_bytes: u64,
    ) -> UpdateResult<()> {
        validate_digest(&self.sha256)?;
        validate_digest(&self.chunk_sha256)?;
        if self.sha256 != request.sha256
            || self.offset != request.offset
            || self.len == 0
            || self.len > request.len
            || self.total_size_bytes != total_size_bytes
            || self.offset.saturating_add(self.len as u64) > total_size_bytes
        {
            return Err(UpdateError::Transfer(
                "peer chunk response does not repeat the requested bounds".to_string(),
            ));
        }
        validate_metadata_size(self)
    }
}

fn validate_digest(value: &str) -> UpdateResult<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(UpdateError::Transfer(
            "peer update digest must be lowercase SHA-256".to_string(),
        ));
    }
    Ok(())
}

fn validate_protocol(value: &str) -> UpdateResult<()> {
    if value.trim().is_empty() || value.len() > 64 || value.chars().any(char::is_control) {
        return Err(UpdateError::Incompatible(
            "peer update protocol version is invalid".to_string(),
        ));
    }
    Ok(())
}

fn validate_metadata_size<T: Serialize>(value: &T) -> UpdateResult<()> {
    let size = serde_json::to_vec(value)
        .map_err(|error| UpdateError::Transfer(error.to_string()))?
        .len();
    if size > UPDATE_PEER_METADATA_MAX_BYTES {
        return Err(UpdateError::Transfer(
            "peer update metadata exceeds configured limit".to_string(),
        ));
    }
    Ok(())
}
