// =============================================================================
//        #######
//     ###       ###     F: tenant.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/26 08:53:09 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:48:56 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Tenant-isolated state partition.

use crate::config::{
    MAX_GATEWAY_CLIENTS_PER_TENANT, MAX_GATEWAY_PENDING_PER_TENANT, MAX_GATEWAY_WORKERS_PER_TENANT,
};
use crate::connection::{ClientConnection, WorkerConnection, WorkerConnectionKey};
use crate::error::GatewayError;
use crate::error::GatewayResult;
use crate::mesh::MeshPeerResponse;
use crate::registry::CapabilityRegistry;
use crate::resolver::CapabilityResolver;
use crate::session::GatewaySession;
use appcore_contracts::InstallationId;
use appcore_distributed_contracts::PeerRpcResponse;
use appcore_types::{CapabilityName, ClusterId, CoreId, TenantId};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use tokio::sync::oneshot;

type WorkerMapKey = (InstallationId, CoreId);
type WorkerTarget = (ClusterId, CoreId);

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkerIndexEntry {
    map_key: WorkerMapKey,
    generation: u64,
}

#[derive(Debug)]
enum PendingSender {
    Peer(oneshot::Sender<PeerRpcResponse>),
    Mesh(oneshot::Sender<MeshPeerResponse>),
}

#[derive(Debug)]
struct PendingRequest {
    sender: PendingSender,
    worker_generation: u64,
    deadline: Instant,
    response_limit: Option<usize>,
}

/// Container for all connections and capabilities isolated to a single tenant.
///
/// Pending-response entries are router-owned and intentionally private. Use
/// [`Self::pending_request_count`] for bounded observation; embedders must not
/// insert or remove response channels directly.
#[derive(Debug)]
pub struct TenantState {
    /// Tenant identifier.
    pub tenant_id: TenantId,

    /// Connected workers belonging to this tenant.
    pub workers: HashMap<WorkerMapKey, WorkerConnection>,

    /// Connected clients belonging to this tenant.
    pub clients: HashMap<String, ClientConnection>,

    /// Advertised capabilities registry.
    pub registry: CapabilityRegistry,

    /// Capability resolver.
    pub resolver: CapabilityResolver,

    /// Active sessions for this tenant.
    pub sessions: HashMap<String, GatewaySession>,

    worker_by_core: HashMap<CoreId, WorkerIndexEntry>,
    worker_by_target: HashMap<WorkerTarget, WorkerIndexEntry>,
    worker_index_rebuilds: u64,
    worker_index_inconsistencies: AtomicU64,
    pending_requests: HashMap<String, PendingRequest>,
}

impl TenantState {
    /// Creates a new tenant state.
    pub fn new(tenant_id: TenantId) -> Self {
        Self {
            tenant_id,
            workers: HashMap::new(),
            clients: HashMap::new(),
            registry: CapabilityRegistry::new(),
            resolver: CapabilityResolver::new(),
            sessions: HashMap::new(),
            worker_by_core: HashMap::new(),
            worker_by_target: HashMap::new(),
            worker_index_rebuilds: 0,
            worker_index_inconsistencies: AtomicU64::new(0),
            pending_requests: HashMap::new(),
        }
    }

    /// Registers a worker connection and advertises its capabilities.
    pub fn add_worker(
        &mut self,
        conn: WorkerConnection,
        capabilities: Vec<CapabilityName>,
    ) -> GatewayResult<()> {
        let key = conn.key.clone();
        let map_key = (key.installation_id.clone(), key.core_id.clone());
        if !self.workers.contains_key(&map_key)
            && self.workers.len() >= MAX_GATEWAY_WORKERS_PER_TENANT
        {
            return Err(GatewayError::Transport(
                "worker connection limit reached".to_string(),
            ));
        }
        if let Some(previous) = self.workers.remove(&map_key) {
            self.registry.deregister(&previous.key);
            self.cancel_pending_for_generation(previous.generation());
            self.remove_worker_indexes(&map_key, &previous);
        }
        self.workers.insert(map_key.clone(), conn);
        self.registry.register(key, capabilities);
        self.index_worker(map_key);
        Ok(())
    }

    /// Removes a worker connection and cleans up its capability registry entries.
    pub fn remove_worker(&mut self, installation_id: &InstallationId, core_id: &CoreId) {
        let map_key = (installation_id.clone(), core_id.clone());
        if let Some(conn) = self.workers.remove(&map_key) {
            self.registry.deregister(&conn.key);
            self.cancel_pending_for_generation(conn.generation());
            self.remove_worker_indexes(&map_key, &conn);
        }
    }

    pub(crate) fn remove_worker_if_current(
        &mut self,
        installation_id: &InstallationId,
        core_id: &CoreId,
        generation: u64,
    ) -> bool {
        let map_key = (installation_id.clone(), core_id.clone());
        let is_current = self
            .workers
            .get(&map_key)
            .is_some_and(|worker| worker.generation() == generation);
        if !is_current {
            self.cancel_pending_for_generation(generation);
            return false;
        }
        self.remove_worker(installation_id, core_id);
        true
    }

    /// Registers a client connection.
    pub fn add_client(&mut self, conn: ClientConnection) {
        self.clients.insert(conn.connection_id.clone(), conn);
    }

    pub(crate) fn try_add_client(&mut self, conn: ClientConnection) -> GatewayResult<()> {
        if !self.clients.contains_key(&conn.connection_id)
            && self.clients.len() >= MAX_GATEWAY_CLIENTS_PER_TENANT
        {
            return Err(GatewayError::Transport(
                "client connection limit reached".to_string(),
            ));
        }
        self.add_client(conn);
        Ok(())
    }

    /// Removes a client connection.
    pub fn remove_client(&mut self, connection_id: &str) {
        self.clients.remove(connection_id);
    }

    /// Resolves a worker key for a given capability.
    pub fn resolve_worker(&self, capability: &CapabilityName) -> Option<WorkerConnectionKey> {
        self.resolver.resolve(capability, &self.registry)
    }

    /// Fetches a worker connection by key.
    pub fn get_worker(
        &self,
        installation_id: &InstallationId,
        core_id: &CoreId,
    ) -> Option<&WorkerConnection> {
        self.workers
            .get(&(installation_id.clone(), core_id.clone()))
    }

    /// Fetches any worker connection by Core ID within this tenant.
    pub fn get_worker_by_core(&self, core_id: &CoreId) -> Option<&WorkerConnection> {
        self.worker_by_core
            .get(core_id)
            .and_then(|entry| self.indexed_worker(entry))
    }

    /// Fetches a worker by its cluster and Core ID within this tenant.
    pub fn get_worker_in_cluster(
        &self,
        cluster_id: &ClusterId,
        core_id: &CoreId,
    ) -> Option<&WorkerConnection> {
        self.worker_by_target
            .get(&(cluster_id.clone(), core_id.clone()))
            .and_then(|entry| self.indexed_worker(entry))
    }

    /// Returns the bounded number of direct worker-index rebuilds.
    pub fn worker_index_rebuilds(&self) -> u64 {
        self.worker_index_rebuilds
    }

    /// Returns the bounded number of detected worker-index inconsistencies.
    pub fn worker_index_inconsistencies(&self) -> u64 {
        self.worker_index_inconsistencies.load(Ordering::Relaxed)
    }

    fn indexed_worker(&self, entry: &WorkerIndexEntry) -> Option<&WorkerConnection> {
        let worker = self
            .workers
            .get(&entry.map_key)
            .filter(|worker| worker.generation() == entry.generation);
        if worker.is_none() {
            saturating_increment(&self.worker_index_inconsistencies);
        }
        worker
    }

    fn index_worker(&mut self, map_key: WorkerMapKey) {
        let Some(worker) = self.workers.get(&map_key) else {
            saturating_increment(&self.worker_index_inconsistencies);
            return;
        };
        let entry = WorkerIndexEntry {
            map_key,
            generation: worker.generation(),
        };
        self.worker_by_core
            .insert(worker.key.core_id.clone(), entry.clone());
        if let Some(cluster_id) = worker.cluster_id() {
            self.worker_by_target
                .insert((cluster_id.clone(), worker.key.core_id.clone()), entry);
        }
    }

    fn remove_worker_indexes(&mut self, map_key: &WorkerMapKey, worker: &WorkerConnection) {
        let generation = worker.generation();
        let core_id = &worker.key.core_id;
        if self
            .worker_by_core
            .get(core_id)
            .is_some_and(|entry| entry.map_key == *map_key && entry.generation == generation)
        {
            self.worker_by_core.remove(core_id);
            self.rebuild_core_index(core_id);
        }
        if let Some(cluster_id) = worker.cluster_id() {
            let target = (cluster_id.clone(), core_id.clone());
            if self
                .worker_by_target
                .get(&target)
                .is_some_and(|entry| entry.map_key == *map_key && entry.generation == generation)
            {
                self.worker_by_target.remove(&target);
                self.rebuild_target_index(&target);
            }
        }
    }

    fn rebuild_core_index(&mut self, core_id: &CoreId) {
        self.worker_index_rebuilds = self.worker_index_rebuilds.saturating_add(1);
        if let Some((map_key, worker)) = self
            .workers
            .iter()
            .filter(|((_, candidate), _)| candidate == core_id)
            .max_by_key(|(_, worker)| worker.generation())
        {
            self.worker_by_core.insert(
                core_id.clone(),
                WorkerIndexEntry {
                    map_key: map_key.clone(),
                    generation: worker.generation(),
                },
            );
        }
    }

    fn rebuild_target_index(&mut self, target: &WorkerTarget) {
        self.worker_index_rebuilds = self.worker_index_rebuilds.saturating_add(1);
        if let Some((map_key, worker)) = self
            .workers
            .iter()
            .filter(|((_, core_id), worker)| {
                core_id == &target.1 && worker.cluster_id() == Some(&target.0)
            })
            .max_by_key(|(_, worker)| worker.generation())
        {
            self.worker_by_target.insert(
                target.clone(),
                WorkerIndexEntry {
                    map_key: map_key.clone(),
                    generation: worker.generation(),
                },
            );
        }
    }

    /// Returns the total pending request count owned by this tenant.
    pub fn pending_request_count(&self) -> usize {
        self.pending_requests.len()
    }

    pub(crate) fn register_pending_request(
        &mut self,
        request_id: String,
        worker_generation: u64,
        deadline: Instant,
    ) -> GatewayResult<oneshot::Receiver<PeerRpcResponse>> {
        let (sender, receiver) = oneshot::channel();
        self.register_pending(
            request_id,
            PendingSender::Peer(sender),
            worker_generation,
            deadline,
            None,
        )?;
        Ok(receiver)
    }

    pub(crate) fn register_pending_mesh_request(
        &mut self,
        request_id: String,
        worker_generation: u64,
        deadline: Instant,
        response_limit: usize,
    ) -> GatewayResult<oneshot::Receiver<MeshPeerResponse>> {
        let (sender, receiver) = oneshot::channel();
        self.register_pending(
            request_id,
            PendingSender::Mesh(sender),
            worker_generation,
            deadline,
            Some(response_limit),
        )?;
        Ok(receiver)
    }

    fn register_pending(
        &mut self,
        request_id: String,
        sender: PendingSender,
        worker_generation: u64,
        deadline: Instant,
        response_limit: Option<usize>,
    ) -> GatewayResult<()> {
        if self.pending_requests.len() >= MAX_GATEWAY_PENDING_PER_TENANT
            || self.pending_requests.contains_key(&request_id)
        {
            return Err(GatewayError::Transport(
                "pending request rejected".to_string(),
            ));
        }
        self.pending_requests.insert(
            request_id,
            PendingRequest {
                sender,
                worker_generation,
                deadline,
                response_limit,
            },
        );
        Ok(())
    }

    pub(crate) fn remove_pending_request(&mut self, request_id: &str) {
        self.pending_requests.remove(request_id);
    }

    pub(crate) fn complete_pending_request(
        &mut self,
        request_id: &str,
        generation: Option<u64>,
        response: PeerRpcResponse,
    ) -> bool {
        let Some(pending) = self.take_pending(request_id, generation) else {
            return false;
        };
        match pending.sender {
            PendingSender::Peer(sender) => {
                let _ = sender.send(response);
                true
            }
            PendingSender::Mesh(_) => false,
        }
    }

    pub(crate) fn complete_pending_mesh_request(
        &mut self,
        request_id: &str,
        generation: Option<u64>,
        response: MeshPeerResponse,
    ) -> bool {
        let Some(pending) = self.take_pending(request_id, generation) else {
            return false;
        };
        let Some(response_limit) = pending.response_limit else {
            return false;
        };
        if response
            .validate_for_request(request_id, response_limit)
            .is_err()
        {
            return false;
        }
        match pending.sender {
            PendingSender::Mesh(sender) => {
                let _ = sender.send(response);
                true
            }
            PendingSender::Peer(_) => false,
        }
    }

    pub(crate) fn cancel_pending_for_generation(&mut self, generation: u64) -> usize {
        let before = self.pending_requests.len();
        self.pending_requests
            .retain(|_, request| request.worker_generation != generation);
        before.saturating_sub(self.pending_requests.len())
    }

    fn take_pending(
        &mut self,
        request_id: &str,
        generation: Option<u64>,
    ) -> Option<PendingRequest> {
        let entry = self.pending_requests.get(request_id)?;
        if entry.deadline <= Instant::now() {
            self.pending_requests.remove(request_id);
            return None;
        }
        if generation.is_some_and(|value| value != entry.worker_generation) {
            return None;
        }
        self.pending_requests.remove(request_id)
    }
}

fn saturating_increment(counter: &AtomicU64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        Some(value.saturating_add(1))
    });
}
