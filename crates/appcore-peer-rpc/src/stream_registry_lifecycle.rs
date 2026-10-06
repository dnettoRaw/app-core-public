// =============================================================================
//        #######
//     ###       ###     F: stream_registry_lifecycle.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/05 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/05 00:00:00 by dnettoRaw
//      ###########      S: 1.0.4-rc
// =============================================================================

//! Expiry cleanup, reservation release and read-only stream observations.

use super::*;

impl PeerRpcStreamRegistry {
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

    pub(super) fn remove_session(
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
}
