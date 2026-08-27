// =============================================================================
//        #######
//     ###       ###     F: binary_codec.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================

//! Explicit bounded binary codec for Peer RPC V2 frames and replies.

use super::{
    PeerRpcStreamCancelV2, PeerRpcStreamChunkV2, PeerRpcStreamCommitV2, PeerRpcStreamFrameV2,
    PeerRpcStreamOpenV2, PeerRpcStreamPullV2, PeerRpcStreamReplyV2,
};
use postcard::ser_flavors::Size;
use serde::{Deserialize, Serialize};

const MAGIC: [u8; 8] = *b"APCRPC2B";
const HEADER_BYTES: usize = 14;
const FRAME_KIND: u8 = 1;
const REPLY_KIND: u8 = 2;

/// Version of the binary codec carried inside the Peer RPC V2 boundary.
pub const PEER_RPC_BINARY_CODEC_VERSION_V2: u8 = 1;
/// Exact media type required by the binary Peer RPC V2 routes.
pub const PEER_RPC_BINARY_CONTENT_TYPE_V2: &str = "application/vnd.appcore.peer-rpc.v2+postcard";
/// Absolute encoded size ceiling for one binary frame or reply.
pub const MAX_PEER_RPC_BINARY_FRAME_BYTES_V2: usize = 256 * 1024;

/// Explicit frame codec selected by both V2 peers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerRpcStreamCodecV2 {
    /// Existing canonical JSON frame with base64 payload bytes.
    Json,
    /// Versioned Postcard frame with native byte strings.
    Binary,
}

/// Controlled failure while encoding or decoding one binary V2 exchange.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PeerRpcBinaryCodecErrorV2 {
    /// The caller supplied a zero limit.
    #[error("peer RPC binary codec limit is invalid")]
    InvalidLimit,
    /// The encoded body exceeds the caller or protocol ceiling.
    #[error("peer RPC binary frame is too large")]
    FrameTooLarge,
    /// The binary framing marker is absent or invalid.
    #[error("peer RPC binary frame marker is invalid")]
    InvalidMagic,
    /// The binary codec version is unsupported.
    #[error("peer RPC binary codec version is unsupported")]
    UnsupportedVersion,
    /// A frame was decoded as a reply or a reply as a frame.
    #[error("peer RPC binary message kind is invalid")]
    InvalidMessageKind,
    /// The declared payload length does not match the exact body.
    #[error("peer RPC binary frame length is invalid")]
    InvalidLength,
    /// The bounded Postcard payload is malformed.
    #[error("peer RPC binary payload is invalid")]
    InvalidPayload,
}

/// Encodes one V2 frame under the smaller caller or protocol size limit.
pub fn encode_binary_frame_v2(
    frame: &PeerRpcStreamFrameV2,
    max_bytes: usize,
) -> Result<Vec<u8>, PeerRpcBinaryCodecErrorV2> {
    encode_message(&BinaryFrameRef::from(frame), FRAME_KIND, max_bytes)
}

/// Decodes one exact V2 frame under the smaller caller or protocol size limit.
pub fn decode_binary_frame_v2(
    body: &[u8],
    max_bytes: usize,
) -> Result<PeerRpcStreamFrameV2, PeerRpcBinaryCodecErrorV2> {
    decode_message::<BinaryFrameOwned>(body, FRAME_KIND, max_bytes).map(Into::into)
}

/// Encodes one V2 reply under the smaller caller or protocol size limit.
pub fn encode_binary_reply_v2(
    reply: &PeerRpcStreamReplyV2,
    max_bytes: usize,
) -> Result<Vec<u8>, PeerRpcBinaryCodecErrorV2> {
    encode_message(&BinaryReplyRef::from(reply), REPLY_KIND, max_bytes)
}

/// Decodes one exact V2 reply under the smaller caller or protocol size limit.
pub fn decode_binary_reply_v2(
    body: &[u8],
    max_bytes: usize,
) -> Result<PeerRpcStreamReplyV2, PeerRpcBinaryCodecErrorV2> {
    decode_message::<BinaryReplyOwned>(body, REPLY_KIND, max_bytes).map(Into::into)
}

fn encode_message<T>(
    value: &T,
    message_kind: u8,
    max_bytes: usize,
) -> Result<Vec<u8>, PeerRpcBinaryCodecErrorV2>
where
    T: Serialize + ?Sized,
{
    let limit = effective_limit(max_bytes)?;
    let payload_bytes = postcard::serialize_with_flavor::<T, Size, usize>(value, Size::default())
        .map_err(|_| PeerRpcBinaryCodecErrorV2::InvalidPayload)?;
    let total_bytes = HEADER_BYTES
        .checked_add(payload_bytes)
        .ok_or(PeerRpcBinaryCodecErrorV2::FrameTooLarge)?;
    if total_bytes > limit || payload_bytes > u32::MAX as usize {
        return Err(PeerRpcBinaryCodecErrorV2::FrameTooLarge);
    }
    let mut body = vec![0; total_bytes];
    body[..8].copy_from_slice(&MAGIC);
    body[8] = PEER_RPC_BINARY_CODEC_VERSION_V2;
    body[9] = message_kind;
    body[10..HEADER_BYTES].copy_from_slice(&(payload_bytes as u32).to_be_bytes());
    let encoded = postcard::to_slice(value, &mut body[HEADER_BYTES..])
        .map_err(|_| PeerRpcBinaryCodecErrorV2::InvalidPayload)?;
    if encoded.len() != payload_bytes {
        return Err(PeerRpcBinaryCodecErrorV2::InvalidPayload);
    }
    Ok(body)
}

fn decode_message<T>(
    body: &[u8],
    expected_kind: u8,
    max_bytes: usize,
) -> Result<T, PeerRpcBinaryCodecErrorV2>
where
    T: for<'de> Deserialize<'de>,
{
    let limit = effective_limit(max_bytes)?;
    if body.len() > limit {
        return Err(PeerRpcBinaryCodecErrorV2::FrameTooLarge);
    }
    if body.len() < HEADER_BYTES {
        return Err(PeerRpcBinaryCodecErrorV2::InvalidLength);
    }
    if body[..8] != MAGIC {
        return Err(PeerRpcBinaryCodecErrorV2::InvalidMagic);
    }
    if body[8] != PEER_RPC_BINARY_CODEC_VERSION_V2 {
        return Err(PeerRpcBinaryCodecErrorV2::UnsupportedVersion);
    }
    if body[9] != expected_kind {
        return Err(PeerRpcBinaryCodecErrorV2::InvalidMessageKind);
    }
    let declared = u32::from_be_bytes(
        body[10..HEADER_BYTES]
            .try_into()
            .map_err(|_| PeerRpcBinaryCodecErrorV2::InvalidLength)?,
    ) as usize;
    if HEADER_BYTES.checked_add(declared) != Some(body.len()) {
        return Err(PeerRpcBinaryCodecErrorV2::InvalidLength);
    }
    postcard::from_bytes(&body[HEADER_BYTES..])
        .map_err(|_| PeerRpcBinaryCodecErrorV2::InvalidPayload)
}

fn effective_limit(max_bytes: usize) -> Result<usize, PeerRpcBinaryCodecErrorV2> {
    if max_bytes == 0 {
        return Err(PeerRpcBinaryCodecErrorV2::InvalidLimit);
    }
    Ok(max_bytes.min(MAX_PEER_RPC_BINARY_FRAME_BYTES_V2))
}

#[derive(Serialize)]
enum BinaryFrameRef<'a> {
    Open(&'a PeerRpcStreamOpenV2),
    Chunk(&'a PeerRpcStreamChunkV2),
    Commit(&'a PeerRpcStreamCommitV2),
    Cancel(&'a PeerRpcStreamCancelV2),
    Pull(&'a PeerRpcStreamPullV2),
}

impl<'a> From<&'a PeerRpcStreamFrameV2> for BinaryFrameRef<'a> {
    fn from(frame: &'a PeerRpcStreamFrameV2) -> Self {
        match frame {
            PeerRpcStreamFrameV2::Open(value) => Self::Open(value),
            PeerRpcStreamFrameV2::Chunk(value) => Self::Chunk(value),
            PeerRpcStreamFrameV2::Commit(value) => Self::Commit(value),
            PeerRpcStreamFrameV2::Cancel(value) => Self::Cancel(value),
            PeerRpcStreamFrameV2::Pull(value) => Self::Pull(value),
        }
    }
}

#[derive(Deserialize)]
enum BinaryFrameOwned {
    Open(Box<PeerRpcStreamOpenV2>),
    Chunk(PeerRpcStreamChunkV2),
    Commit(PeerRpcStreamCommitV2),
    Cancel(PeerRpcStreamCancelV2),
    Pull(PeerRpcStreamPullV2),
}

impl From<BinaryFrameOwned> for PeerRpcStreamFrameV2 {
    fn from(frame: BinaryFrameOwned) -> Self {
        match frame {
            BinaryFrameOwned::Open(value) => Self::Open(value),
            BinaryFrameOwned::Chunk(value) => Self::Chunk(value),
            BinaryFrameOwned::Commit(value) => Self::Commit(value),
            BinaryFrameOwned::Cancel(value) => Self::Cancel(value),
            BinaryFrameOwned::Pull(value) => Self::Pull(value),
        }
    }
}

#[derive(Serialize)]
struct BinaryReplyRef<'a> {
    request_id: &'a str,
    stream_id: &'a str,
    next_sequence: u32,
    received_bytes: u64,
    response_frame: Option<BinaryFrameRef<'a>>,
    complete: bool,
}

impl<'a> From<&'a PeerRpcStreamReplyV2> for BinaryReplyRef<'a> {
    fn from(reply: &'a PeerRpcStreamReplyV2) -> Self {
        Self {
            request_id: &reply.request_id,
            stream_id: &reply.stream_id,
            next_sequence: reply.next_sequence,
            received_bytes: reply.received_bytes,
            response_frame: reply.response_frame.as_deref().map(Into::into),
            complete: reply.complete,
        }
    }
}

#[derive(Deserialize)]
struct BinaryReplyOwned {
    request_id: String,
    stream_id: String,
    next_sequence: u32,
    received_bytes: u64,
    response_frame: Option<BinaryFrameOwned>,
    complete: bool,
}

impl From<BinaryReplyOwned> for PeerRpcStreamReplyV2 {
    fn from(reply: BinaryReplyOwned) -> Self {
        Self {
            request_id: reply.request_id,
            stream_id: reply.stream_id,
            next_sequence: reply.next_sequence,
            received_bytes: reply.received_bytes,
            response_frame: reply.response_frame.map(|frame| Box::new(frame.into())),
            complete: reply.complete,
        }
    }
}
