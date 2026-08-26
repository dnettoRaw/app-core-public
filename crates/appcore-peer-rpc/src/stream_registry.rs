// =============================================================================
//        #######
//     ###       ###     F: stream_registry.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================

//! Bounded ownership of partial V2 request and response streams.

use super::*;
use crate::stream_registry_protocol::{reply, response_open};
use crate::stream_registry_types::{
    PeerRpcStreamDispatcherV2, PeerRpcStreamRegistryConfig, PeerRpcStreamRegistrySnapshot,
    PeerRpcStreamResponseSourceV2,
};
use crate::stream_session::PeerRpcStreamSession as Session;
use crate::v2::{
    PeerRpcStreamCancelV2, PeerRpcStreamChunkV2, PeerRpcStreamCommitV2, PeerRpcStreamErrorV2,
    PeerRpcStreamFrameV2, PeerRpcStreamOpenV2, PeerRpcStreamPullV2, PeerRpcStreamReplyV2,
    PEER_RPC_PROTOCOL_VERSION_V2,
};
use std::collections::HashMap;
use std::sync::Mutex;

/// Process-local bounded owner of partial V2 request and response sessions.
pub struct PeerRpcStreamRegistry {
    config: PeerRpcStreamRegistryConfig,
    dispatcher: Arc<dyn PeerRpcStreamDispatcherV2>,
    inner: Mutex<RegistryInner>,
}

#[derive(Default)]
struct RegistryInner {
    sessions: HashMap<String, Arc<Mutex<Session>>>,
    reserved_payload_bytes: u64,
    saturation_count: u64,
    cleanup_count: u64,
}

impl PeerRpcStreamRegistry {
    /// Creates an empty bounded registry and validates its owner-only spool directory.
    pub fn new(
        config: PeerRpcStreamRegistryConfig,
        dispatcher: Arc<dyn PeerRpcStreamDispatcherV2>,
    ) -> Result<Self, PeerRpcStreamErrorV2> {
        if config.max_sessions == 0 || config.max_reserved_payload_bytes == 0 {
            return Err(PeerRpcStreamErrorV2::InvalidConfig);
        }
        let probe = PeerRpcStreamPayload::create(&config.spool_directory)?;
        drop(probe);
        Ok(Self {
            config,
            dispatcher,
            inner: Mutex::new(RegistryInner::default()),
        })
    }

    /// Returns the maximum JSON body admitted for one base64-encoded V2 frame.
    pub fn max_http_frame_bytes(&self) -> usize {
        self.config
            .chunk_limits
            .max_encoded_chunk_bytes
            .saturating_mul(4)
            .div_ceil(3)
            .saturating_add(64 * 1024)
    }

    /// Exchanges one frame on the explicitly selected query or command boundary.
    pub fn exchange(
        &self,
        expected_kind: PeerRpcCallKind,
        frame: PeerRpcStreamFrameV2,
        now_ms: u64,
    ) -> Result<PeerRpcStreamReplyV2, PeerRpcStreamErrorV2> {
        match frame {
            PeerRpcStreamFrameV2::Open(open) => {
                if open.call_kind != expected_kind {
                    return Err(PeerRpcStreamErrorV2::CallKindMismatch);
                }
                self.open(*open, now_ms)
            }
            PeerRpcStreamFrameV2::Chunk(chunk) => {
                self.ensure_call_kind(&chunk.stream_id, &chunk.request_id, expected_kind)?;
                self.push_chunk(chunk, now_ms)
            }
            PeerRpcStreamFrameV2::Commit(commit) => {
                self.ensure_call_kind(&commit.stream_id, &commit.request_id, expected_kind)?;
                self.commit(commit, now_ms)
            }
            PeerRpcStreamFrameV2::Cancel(cancel) => {
                self.ensure_call_kind(&cancel.stream_id, &cancel.request_id, expected_kind)?;
                self.cancel(&cancel)?;
                Ok(reply(
                    &cancel.request_id,
                    &cancel.stream_id,
                    0,
                    0,
                    None,
                    true,
                ))
            }
            PeerRpcStreamFrameV2::Pull(pull) => {
                self.ensure_call_kind(&pull.stream_id, &pull.request_id, expected_kind)?;
                self.pull(pull, now_ms)
            }
        }
    }

    /// Admits one validated open frame and reserves its exact decoded size.
    pub fn open(
        &self,
        open: PeerRpcStreamOpenV2,
        now_ms: u64,
    ) -> Result<PeerRpcStreamReplyV2, PeerRpcStreamErrorV2> {
        if open.direction != crate::v2::PeerRpcStreamDirectionV2::Request {
            return Err(PeerRpcStreamErrorV2::DirectionMismatch);
        }
        self.cleanup_expired(now_ms);
        let cancellation = CancellationToken::new();
        let payload = PeerRpcStreamPayload::create(&self.config.spool_directory)?;
        let assembler = PeerRpcChunkAssembler::new(
            open.clone(),
            payload,
            self.config.chunk_limits.clone(),
            cancellation.clone(),
            now_ms,
        )?;
        let session = Arc::new(Mutex::new(Session::Receiving {
            request_id: open.request_id.clone(),
            call_kind: open.call_kind,
            deadline_ms: open.deadline_ms,
            reserved_bytes: open.payload_bytes,
            cancellation,
            assembler: Some(assembler),
        }));
        let mut inner = lock(&self.inner)?;
        let next_reserved = inner
            .reserved_payload_bytes
            .saturating_add(open.payload_bytes);
        if inner.sessions.len() >= self.config.max_sessions
            || next_reserved > self.config.max_reserved_payload_bytes
        {
            inner.saturation_count = inner.saturation_count.saturating_add(1);
            return Err(PeerRpcStreamErrorV2::CapacityExceeded);
        }
        if inner.sessions.contains_key(&open.stream_id) {
            return Err(PeerRpcStreamErrorV2::IdentityMismatch);
        }
        inner.reserved_payload_bytes = next_reserved;
        inner.sessions.insert(open.stream_id.clone(), session);
        Ok(reply(&open.request_id, &open.stream_id, 0, 0, None, false))
    }

    /// Accepts exactly the next request chunk or removes the failed partial state.
    pub fn push_chunk(
        &self,
        chunk: PeerRpcStreamChunkV2,
        now_ms: u64,
    ) -> Result<PeerRpcStreamReplyV2, PeerRpcStreamErrorV2> {
        let session = self.find_session(&chunk.stream_id)?;
        let result = {
            let mut session = lock(&session)?;
            match &mut *session {
                Session::Receiving {
                    request_id,
                    assembler,
                    ..
                } if request_id == &chunk.request_id => match assembler.as_mut() {
                    Some(assembler) => assembler.push_chunk(chunk.clone(), now_ms).map(|()| {
                        reply(
                            request_id,
                            &chunk.stream_id,
                            assembler.next_sequence(),
                            assembler.received_bytes(),
                            None,
                            false,
                        )
                    }),
                    None => Err(PeerRpcStreamErrorV2::Closed),
                },
                _ => Err(PeerRpcStreamErrorV2::IdentityMismatch),
            }
        };
        if result.is_err() {
            self.remove_session(&chunk.stream_id, &session)?;
        }
        result
    }

    /// Commits a request, dispatches it, and returns the response open frame.
    pub fn commit(
        &self,
        commit: PeerRpcStreamCommitV2,
        now_ms: u64,
    ) -> Result<PeerRpcStreamReplyV2, PeerRpcStreamErrorV2> {
        let session = self.find_session(&commit.stream_id)?;
        let (open, assembler, cancellation, request_reserved) = {
            let mut session = lock(&session)?;
            let Session::Receiving {
                request_id,
                call_kind,
                deadline_ms,
                reserved_bytes,
                cancellation,
                assembler,
            } = &mut *session
            else {
                return Err(PeerRpcStreamErrorV2::Closed);
            };
            if request_id != &commit.request_id {
                return Err(PeerRpcStreamErrorV2::IdentityMismatch);
            }
            let assembler = assembler.take().ok_or(PeerRpcStreamErrorV2::Closed)?;
            let open = assembler.open().clone();
            let dispatch_request_id = request_id.clone();
            let dispatch_call_kind = *call_kind;
            let dispatch_deadline_ms = *deadline_ms;
            let dispatch_reserved_bytes = *reserved_bytes;
            let dispatch_cancellation = cancellation.clone();
            *session = Session::Dispatching {
                request_id: dispatch_request_id,
                call_kind: dispatch_call_kind,
                deadline_ms: dispatch_deadline_ms,
                reserved_bytes: dispatch_reserved_bytes,
                cancellation: dispatch_cancellation.clone(),
            };
            (
                open,
                assembler,
                dispatch_cancellation,
                dispatch_reserved_bytes,
            )
        };
        let request_stream_id = open.stream_id.clone();
        let mut payload = match assembler.finish(commit, now_ms) {
            Ok(payload) => payload,
            Err(error) => {
                self.remove_session(&open.stream_id, &session)?;
                return Err(error);
            }
        };
        if let Err(error) = payload.rewind() {
            self.remove_session(&open.stream_id, &session)?;
            return Err(error);
        }
        let response =
            self.dispatcher
                .dispatch_peer_stream(open.clone(), payload, cancellation.clone());
        self.complete_dispatch(
            session,
            &request_stream_id,
            open,
            response,
            cancellation,
            request_reserved,
            now_ms,
        )
    }

    /// Returns the next bounded response frame and removes state after commit.
    pub fn pull(
        &self,
        pull: PeerRpcStreamPullV2,
        now_ms: u64,
    ) -> Result<PeerRpcStreamReplyV2, PeerRpcStreamErrorV2> {
        if pull.protocol_version.as_u16() != PEER_RPC_PROTOCOL_VERSION_V2 {
            return Err(PeerRpcStreamErrorV2::ProtocolMismatch);
        }
        let session = self.find_session(&pull.stream_id)?;
        let frame_result = {
            let mut session = lock(&session)?;
            match &mut *session {
                Session::Responding {
                    request_id,
                    encoder,
                    ..
                } if request_id == &pull.request_id => encoder.next_frame(now_ms),
                _ => return Err(PeerRpcStreamErrorV2::IdentityMismatch),
            }
        };
        let frame = match frame_result {
            Ok(frame) => frame,
            Err(error) => {
                self.remove_session(&pull.stream_id, &session)?;
                return Err(error);
            }
        };
        let complete = matches!(frame, Some(PeerRpcStreamFrameV2::Commit(_)) | None);
        if complete {
            self.remove_session(&pull.stream_id, &session)?;
        }
        Ok(reply(
            &pull.request_id,
            &pull.stream_id,
            0,
            0,
            frame.map(Box::new),
            complete,
        ))
    }

    /// Cancels a request, dispatch, or response stream and releases owned state.
    pub fn cancel(&self, cancel: &PeerRpcStreamCancelV2) -> Result<bool, PeerRpcStreamErrorV2> {
        if cancel.protocol_version.as_u16() != PEER_RPC_PROTOCOL_VERSION_V2 {
            return Err(PeerRpcStreamErrorV2::ProtocolMismatch);
        }
        let Ok(session) = self.find_session(&cancel.stream_id) else {
            return Ok(false);
        };
        let dispatching = {
            let session = lock(&session)?;
            if session.request_id() != cancel.request_id {
                return Err(PeerRpcStreamErrorV2::IdentityMismatch);
            }
            session.cancellation().cancel();
            matches!(*session, Session::Dispatching { .. })
        };
        if !dispatching {
            self.remove_session(&cancel.stream_id, &session)?;
        }
        Ok(true)
    }

    /// Removes expired partial request/response state and cancels active dispatches.
    pub fn cleanup_expired(&self, now_ms: u64) -> usize {
        let sessions = match self.inner.lock() {
            Ok(inner) => inner
                .sessions
                .iter()
                .map(|(id, session)| (id.clone(), Arc::clone(session)))
                .collect::<Vec<_>>(),
            Err(_) => return 0,
        };
        let mut removed = 0usize;
        for (id, session) in sessions {
            let (expired, dispatching) = match session.lock() {
                Ok(session) => {
                    let expired = now_ms >= session.deadline_ms();
                    if expired {
                        session.cancellation().cancel();
                    }
                    (expired, matches!(*session, Session::Dispatching { .. }))
                }
                Err(_) => (true, false),
            };
            if expired && !dispatching && self.remove_session(&id, &session).is_ok() {
                removed = removed.saturating_add(1);
            }
        }
        removed
    }

    /// Returns bounded session, reservation, saturation, and cleanup observations.
    pub fn snapshot(&self) -> Result<PeerRpcStreamRegistrySnapshot, PeerRpcStreamErrorV2> {
        let inner = lock(&self.inner)?;
        Ok(PeerRpcStreamRegistrySnapshot {
            active_sessions: inner.sessions.len(),
            reserved_payload_bytes: inner.reserved_payload_bytes,
            saturation_count: inner.saturation_count,
            cleanup_count: inner.cleanup_count,
        })
    }

    fn find_session(&self, stream_id: &str) -> Result<Arc<Mutex<Session>>, PeerRpcStreamErrorV2> {
        lock(&self.inner)?
            .sessions
            .get(stream_id)
            .cloned()
            .ok_or(PeerRpcStreamErrorV2::IdentityMismatch)
    }

    fn ensure_call_kind(
        &self,
        stream_id: &str,
        request_id: &str,
        expected_kind: PeerRpcCallKind,
    ) -> Result<(), PeerRpcStreamErrorV2> {
        let session = self.find_session(stream_id)?;
        let session = lock(&session)?;
        if session.request_id() != request_id {
            return Err(PeerRpcStreamErrorV2::IdentityMismatch);
        }
        if session.call_kind() != expected_kind {
            return Err(PeerRpcStreamErrorV2::CallKindMismatch);
        }
        Ok(())
    }

    fn remove_session(
        &self,
        stream_id: &str,
        expected: &Arc<Mutex<Session>>,
    ) -> Result<(), PeerRpcStreamErrorV2> {
        let mut inner = lock(&self.inner)?;
        let matches = inner
            .sessions
            .get(stream_id)
            .is_some_and(|session| Arc::ptr_eq(session, expected));
        if matches {
            if let Some(session) = inner.sessions.remove(stream_id) {
                let session = session
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                inner.reserved_payload_bytes = inner
                    .reserved_payload_bytes
                    .saturating_sub(session.reserved_bytes());
            }
            inner.cleanup_count = inner.cleanup_count.saturating_add(1);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn complete_dispatch(
        &self,
        session: Arc<Mutex<Session>>,
        request_stream_id: &str,
        open: PeerRpcStreamOpenV2,
        response: Result<PeerRpcStreamResponseSourceV2, PeerRpcStreamErrorV2>,
        cancellation: CancellationToken,
        request_reserved: u64,
        now_ms: u64,
    ) -> Result<PeerRpcStreamReplyV2, PeerRpcStreamErrorV2> {
        let response = match response {
            Ok(response) if !cancellation.is_cancelled() => response,
            Ok(_) => {
                self.remove_session(request_stream_id, &session)?;
                return Err(PeerRpcStreamErrorV2::Cancelled);
            }
            Err(error) => {
                self.remove_session(request_stream_id, &session)?;
                return Err(error);
            }
        };
        let response_bytes = response.payload_bytes;
        let prepared =
            response_open(&open, response_bytes, &self.config, now_ms).and_then(|response_open| {
                let response_stream_id = response_open.stream_id.clone();
                let mut encoder = PeerRpcChunkEncoder::new(
                    response_open,
                    response.reader,
                    self.config.chunk_limits.clone(),
                    cancellation.clone(),
                    now_ms,
                )?;
                let first = encoder.next_frame(now_ms)?;
                Ok((response_stream_id, encoder, first))
            });
        let (response_stream_id, encoder, first) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                self.remove_session(request_stream_id, &session)?;
                return Err(error);
            }
        };
        let mut inner = lock(&self.inner)?;
        let next_reserved = inner
            .reserved_payload_bytes
            .saturating_sub(request_reserved)
            .saturating_add(response_bytes);
        if next_reserved > self.config.max_reserved_payload_bytes {
            inner.saturation_count = inner.saturation_count.saturating_add(1);
            inner.sessions.remove(request_stream_id);
            inner.reserved_payload_bytes = inner
                .reserved_payload_bytes
                .saturating_sub(request_reserved);
            inner.cleanup_count = inner.cleanup_count.saturating_add(1);
            return Err(PeerRpcStreamErrorV2::CapacityExceeded);
        }
        inner.sessions.remove(request_stream_id);
        inner.sessions.insert(
            response_stream_id.clone(),
            Arc::new(Mutex::new(Session::Responding {
                request_id: open.request_id.clone(),
                call_kind: open.call_kind,
                deadline_ms: open.deadline_ms,
                reserved_bytes: response_bytes,
                cancellation,
                encoder,
            })),
        );
        inner.reserved_payload_bytes = next_reserved;
        Ok(reply(
            &open.request_id,
            &response_stream_id,
            0,
            open.payload_bytes,
            first.map(Box::new),
            false,
        ))
    }
}

fn lock<T>(mutex: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>, PeerRpcStreamErrorV2> {
    mutex.lock().map_err(|_| PeerRpcStreamErrorV2::Closed)
}
