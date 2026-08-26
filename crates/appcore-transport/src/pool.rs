// =============================================================================
//        #######
//     ###       ###     F: pool.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.0
// =============================================================================

//! Bounded per-origin connection admission and idle ownership.

use crate::connection::TransportConnection;
use crate::{
    CancellationToken, HttpPoolConfig, HttpScheme, HttpTarget, TransportError, TransportResult,
};
use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Origin {
    scheme: u8,
    host: String,
    port: u16,
}

impl Origin {
    pub(crate) fn from_target(target: &HttpTarget) -> Self {
        Self {
            scheme: match target.scheme() {
                HttpScheme::Http => 0,
                HttpScheme::Https => 1,
            },
            host: target.host().to_string(),
            port: target.port(),
        }
    }
}

struct IdleConnection {
    connection: TransportConnection,
    since: Instant,
}

struct OriginState {
    active: usize,
    idle: Vec<IdleConnection>,
    last_used: Instant,
}

impl OriginState {
    fn new() -> Self {
        Self {
            active: 0,
            idle: Vec::new(),
            last_used: Instant::now(),
        }
    }
}

#[derive(Default)]
struct PoolState {
    origins: HashMap<Origin, OriginState>,
}

pub(crate) struct ConnectionPool {
    config: HttpPoolConfig,
    state: Mutex<PoolState>,
    available: Condvar,
}

impl ConnectionPool {
    pub(crate) fn new(config: HttpPoolConfig) -> Arc<Self> {
        Arc::new(Self {
            config,
            state: Mutex::new(PoolState::default()),
            available: Condvar::new(),
        })
    }

    pub(crate) fn acquire(
        self: &Arc<Self>,
        origin: Origin,
        timeout_ms: u64,
        cancellation: Option<&CancellationToken>,
    ) -> TransportResult<ConnectionLease> {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
        let mut state = self.lock_state();
        loop {
            if cancellation.is_some_and(CancellationToken::is_cancelled) {
                return Err(TransportError::Cancelled);
            }
            self.prune(&mut state);
            if !self.ensure_origin(&mut state, &origin) {
                state = self.wait_for_slot(state, deadline)?;
                continue;
            }
            let Some(entry) = state.origins.get_mut(&origin) else {
                return Err(TransportError::Timeout);
            };
            if let Some(idle) = entry.idle.pop() {
                entry.active = entry.active.saturating_add(1);
                entry.last_used = Instant::now();
                return Ok(ConnectionLease::new(
                    Arc::clone(self),
                    origin,
                    Some(idle.connection),
                ));
            }
            if entry.active < self.config.max_connections_per_origin {
                entry.active += 1;
                entry.last_used = Instant::now();
                return Ok(ConnectionLease::new(Arc::clone(self), origin, None));
            }
            state = self.wait_for_slot(state, deadline)?;
        }
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, PoolState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn ensure_origin(&self, state: &mut PoolState, origin: &Origin) -> bool {
        if state.origins.contains_key(origin) {
            return true;
        }
        if state.origins.len() >= self.config.max_origins {
            let candidate = state
                .origins
                .iter()
                .filter(|(_, entry)| entry.active == 0)
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| key.clone());
            let Some(candidate) = candidate else {
                return false;
            };
            state.origins.remove(&candidate);
        }
        state.origins.insert(origin.clone(), OriginState::new());
        true
    }

    fn wait_for_slot<'a>(
        &self,
        state: MutexGuard<'a, PoolState>,
        deadline: Instant,
    ) -> TransportResult<MutexGuard<'a, PoolState>> {
        let now = Instant::now();
        if now >= deadline {
            return Err(TransportError::Timeout);
        }
        let wait = deadline
            .saturating_duration_since(now)
            .min(Duration::from_millis(25));
        Ok(self
            .available
            .wait_timeout(state, wait)
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .0)
    }

    fn prune(&self, state: &mut PoolState) {
        let now = Instant::now();
        let idle_timeout = Duration::from_millis(self.config.idle_timeout_ms.max(1));
        for entry in state.origins.values_mut() {
            entry
                .idle
                .retain(|idle| now.duration_since(idle.since) < idle_timeout);
        }
        state.origins.retain(|_, entry| {
            entry.active > 0
                || !entry.idle.is_empty()
                || now.duration_since(entry.last_used) < idle_timeout
        });
    }

    fn release(&self, origin: &Origin, connection: Option<TransportConnection>) {
        let mut state = self.lock_state();
        if let Some(entry) = state.origins.get_mut(origin) {
            entry.active = entry.active.saturating_sub(1);
            entry.last_used = Instant::now();
            if let Some(connection) = connection {
                if entry.idle.len() < self.config.max_idle_per_origin {
                    entry.idle.push(IdleConnection {
                        connection,
                        since: Instant::now(),
                    });
                }
            }
        }
        self.available.notify_one();
    }
}

pub(crate) struct ConnectionLease {
    pool: Arc<ConnectionPool>,
    origin: Origin,
    connection: Option<TransportConnection>,
}

impl ConnectionLease {
    fn new(
        pool: Arc<ConnectionPool>,
        origin: Origin,
        connection: Option<TransportConnection>,
    ) -> Self {
        Self {
            pool,
            origin,
            connection,
        }
    }

    pub(crate) fn take(&mut self) -> Option<TransportConnection> {
        self.connection.take()
    }

    pub(crate) fn keep(&mut self, connection: TransportConnection) {
        self.connection = Some(connection);
    }
}

impl Drop for ConnectionLease {
    fn drop(&mut self) {
        self.pool.release(&self.origin, self.connection.take());
    }
}
