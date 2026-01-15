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
use appcore_types::{CapabilityName, CoreId, TenantId};
use std::collections::HashMap;
use tokio::sync::oneshot;

/// Container for all connections and capabilities isolated to a single tenant.
#[derive(Debug)]
pub struct TenantState {
    /// Tenant identifier.
    pub tenant_id: TenantId,

    /// Connected workers belonging to this tenant.
    pub workers: HashMap<(InstallationId, CoreId), WorkerConnection>,

    /// Connected clients belonging to this tenant.
    pub clients: HashMap<String, ClientConnection>,

    /// Advertised capabilities registry.
    pub registry: CapabilityRegistry,

    /// Capability resolver.
    pub resolver: CapabilityResolver,

    /// Active sessions for this tenant.
    pub sessions: HashMap<String, GatewaySession>,

    /// Pending requests waiting for responses from workers (request_id -> response sender).
    pub pending_requests: HashMap<String, oneshot::Sender<PeerRpcResponse>>,

    /// Pending mesh relay requests waiting for worker HTTP responses.
    pub pending_mesh_requests: HashMap<String, oneshot::Sender<MeshPeerResponse>>,
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
            pending_requests: HashMap::new(),
            pending_mesh_requests: HashMap::new(),
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
        if let Some(previous) = self.workers.get(&map_key) {
            self.registry.deregister(&previous.key);
        }
        self.workers.insert(map_key, conn);
        self.registry.register(key, capabilities);
        Ok(())
    }

    /// Removes a worker connection and cleans up its capability registry entries.
    pub fn remove_worker(&mut self, installation_id: &InstallationId, core_id: &CoreId) {
        let map_key = (installation_id.clone(), core_id.clone());
        if let Some(conn) = self.workers.remove(&map_key) {
            self.registry.deregister(&conn.key);
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
        self.workers
            .iter()
            .find_map(|((_, candidate), worker)| (candidate == core_id).then_some(worker))
    }

    pub(crate) fn can_register_pending(&self, request_id: &str, mesh: bool) -> bool {
        if mesh {
            self.pending_mesh_requests.len() < MAX_GATEWAY_PENDING_PER_TENANT
                && !self.pending_mesh_requests.contains_key(request_id)
        } else {
            self.pending_requests.len() < MAX_GATEWAY_PENDING_PER_TENANT
                && !self.pending_requests.contains_key(request_id)
        }
    }
}
