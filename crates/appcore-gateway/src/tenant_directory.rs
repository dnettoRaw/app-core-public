// =============================================================================
//        #######
//     ###       ###     F: tenant_directory.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.0
// =============================================================================

//! Sharded ownership of tenant-isolated Gateway state.

use crate::config::MAX_GATEWAY_TENANTS;
use crate::{GatewayError, GatewayResult, TenantState};
use appcore_types::TenantId;
use parking_lot::RwLock;
use std::collections::hash_map::RandomState;
use std::collections::HashMap;
use std::hash::BuildHasher;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

const TENANT_SHARD_COUNT: usize = 32;

/// Independently synchronized state owned by one Gateway tenant partition.
pub type SharedTenantState = Arc<RwLock<TenantState>>;
type TenantShard = Arc<HashMap<TenantId, SharedTenantState>>;

pub(crate) struct TenantDirectorySnapshot {
    shards: [TenantShard; TENANT_SHARD_COUNT],
}

impl TenantDirectorySnapshot {
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&TenantId, &SharedTenantState)> {
        self.shards.iter().flat_map(|shard| shard.iter())
    }
}

pub(crate) struct TenantDirectory {
    shards: Box<[RwLock<TenantShard>]>,
    hash_builder: RandomState,
    tenant_count: AtomicUsize,
}

impl TenantDirectory {
    pub(crate) fn new() -> Self {
        let shards = (0..TENANT_SHARD_COUNT)
            .map(|_| RwLock::new(Arc::new(HashMap::new())))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Self {
            shards,
            hash_builder: RandomState::new(),
            tenant_count: AtomicUsize::new(0),
        }
    }

    pub(crate) fn get(&self, tenant_id: &TenantId) -> Option<SharedTenantState> {
        self.shard(tenant_id).read().get(tenant_id).cloned()
    }

    pub(crate) fn get_or_insert(&self, tenant_id: &TenantId) -> GatewayResult<SharedTenantState> {
        let mut shard = self.shard(tenant_id).write();
        if let Some(tenant) = shard.get(tenant_id) {
            return Ok(Arc::clone(tenant));
        }
        self.reserve_tenant()?;
        let tenant = Arc::new(RwLock::new(TenantState::new(tenant_id.clone())));
        Arc::make_mut(&mut *shard).insert(tenant_id.clone(), Arc::clone(&tenant));
        Ok(tenant)
    }

    pub(crate) fn snapshot(&self) -> TenantDirectorySnapshot {
        // A scan owns only one pointer per shard, so tenant locks are acquired
        // after every directory lock has been released. Rare insertions copy
        // only their bounded shard when a concurrent scan still holds it.
        TenantDirectorySnapshot {
            shards: std::array::from_fn(|index| Arc::clone(&self.shards[index].read())),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.tenant_count.load(Ordering::Acquire)
    }

    pub(crate) fn connection_count(&self) -> usize {
        self.snapshot().iter().fold(0usize, |total, (_, tenant)| {
            let tenant = tenant.read();
            total
                .saturating_add(tenant.workers.len())
                .saturating_add(tenant.clients.len())
        })
    }

    fn reserve_tenant(&self) -> GatewayResult<()> {
        self.tenant_count
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < MAX_GATEWAY_TENANTS).then_some(count + 1)
            })
            .map(|_| ())
            .map_err(|_| GatewayError::Transport("Gateway tenant limit reached".to_string()))
    }

    fn shard(&self, tenant_id: &TenantId) -> &RwLock<TenantShard> {
        let hash = self.hash_builder.hash_one(tenant_id);
        &self.shards[(hash as usize) % self.shards.len()]
    }
}

#[cfg(test)]
#[path = "tenant_directory_tests.rs"]
mod tests;
