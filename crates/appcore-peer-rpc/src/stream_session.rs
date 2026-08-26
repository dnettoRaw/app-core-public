// =============================================================================
//        #######
//     ###       ###     F: stream_session.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================

//! Internal state machine for one admitted V2 request or response stream.

use super::*;
use crate::PeerRpcCallKind;
use std::io::Read;

pub(crate) enum PeerRpcStreamSession {
    Receiving {
        request_id: String,
        call_kind: PeerRpcCallKind,
        deadline_ms: u64,
        reserved_bytes: u64,
        cancellation: CancellationToken,
        assembler: Option<PeerRpcChunkAssembler<PeerRpcStreamPayload>>,
    },
    Dispatching {
        request_id: String,
        call_kind: PeerRpcCallKind,
        deadline_ms: u64,
        reserved_bytes: u64,
        cancellation: CancellationToken,
    },
    Responding {
        request_id: String,
        call_kind: PeerRpcCallKind,
        deadline_ms: u64,
        reserved_bytes: u64,
        cancellation: CancellationToken,
        encoder: PeerRpcChunkEncoder<Box<dyn Read + Send>>,
    },
}

impl PeerRpcStreamSession {
    pub(crate) fn request_id(&self) -> &str {
        match self {
            Self::Receiving { request_id, .. }
            | Self::Dispatching { request_id, .. }
            | Self::Responding { request_id, .. } => request_id,
        }
    }

    pub(crate) fn call_kind(&self) -> PeerRpcCallKind {
        match self {
            Self::Receiving { call_kind, .. }
            | Self::Dispatching { call_kind, .. }
            | Self::Responding { call_kind, .. } => *call_kind,
        }
    }

    pub(crate) fn deadline_ms(&self) -> u64 {
        match self {
            Self::Receiving { deadline_ms, .. }
            | Self::Dispatching { deadline_ms, .. }
            | Self::Responding { deadline_ms, .. } => *deadline_ms,
        }
    }

    pub(crate) fn reserved_bytes(&self) -> u64 {
        match self {
            Self::Receiving { reserved_bytes, .. }
            | Self::Dispatching { reserved_bytes, .. }
            | Self::Responding { reserved_bytes, .. } => *reserved_bytes,
        }
    }

    pub(crate) fn cancellation(&self) -> &CancellationToken {
        match self {
            Self::Receiving { cancellation, .. }
            | Self::Dispatching { cancellation, .. }
            | Self::Responding { cancellation, .. } => cancellation,
        }
    }
}
