// =============================================================================
//        #######
//     ###       ###     F: coordinator_support.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.6-rc
// =============================================================================

//! Internal bounded-round helpers for the HA coordinator.

use super::coordinator::{CoordinatorOwnership, RecoveredOwnership};
use super::{
    GatewayInstanceLease, GatewayRegistryError, GatewayRegistryFuture, GatewayRegistryResult,
    MAX_GATEWAY_REGISTRY_CONCURRENCY, MAX_GATEWAY_REGISTRY_OPERATION_TIMEOUT_MS,
};
use appcore_types::TenantId;
use futures_util::{stream, StreamExt};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(super) fn duration_ms(duration: Duration) -> GatewayRegistryResult<u64> {
    u64::try_from(duration.as_millis())
        .ok()
        .filter(|value| *value > 0)
        .ok_or(GatewayRegistryError::InvalidContract)
}

pub(super) fn operation_round_timeout() -> Duration {
    Duration::from_millis(MAX_GATEWAY_REGISTRY_OPERATION_TIMEOUT_MS)
}

pub(super) fn find_lease<'a>(
    leases: &'a [GatewayInstanceLease],
    tenant_id: &TenantId,
) -> Option<&'a GatewayInstanceLease> {
    leases.iter().find(|lease| lease.tenant_id() == tenant_id)
}

pub(super) async fn replay_operations<'a>(
    operations: Vec<GatewayRegistryFuture<'a, RecoveredOwnership>>,
) -> GatewayRegistryResult<Vec<GatewayRegistryResult<RecoveredOwnership>>> {
    let results = stream::iter(operations)
        .buffer_unordered(MAX_GATEWAY_REGISTRY_CONCURRENCY)
        .collect::<Vec<_>>();
    tokio::time::timeout(operation_round_timeout(), results)
        .await
        .map_err(|_| GatewayRegistryError::Unavailable)
}

pub(super) fn collect_replayed(
    results: GatewayRegistryResult<Vec<GatewayRegistryResult<RecoveredOwnership>>>,
    leases: Vec<GatewayInstanceLease>,
) -> Result<CoordinatorOwnership, (GatewayRegistryError, Vec<GatewayInstanceLease>)> {
    let results = match results {
        Ok(results) => results,
        Err(error) => return Err((error, leases)),
    };
    let mut ownership = CoordinatorOwnership {
        leases,
        ..CoordinatorOwnership::default()
    };
    for result in results {
        match result {
            Ok(RecoveredOwnership::Worker(worker)) => ownership.workers.push(worker),
            Ok(RecoveredOwnership::Session(session)) => ownership.sessions.push(session),
            Err(error) => return Err((error, ownership.leases)),
        }
    }
    Ok(ownership)
}

pub(super) fn collect_acquired(
    results: Vec<GatewayRegistryResult<GatewayInstanceLease>>,
) -> Result<Vec<GatewayInstanceLease>, (GatewayRegistryError, Vec<GatewayInstanceLease>)> {
    let mut acquired = Vec::with_capacity(results.len());
    let mut first_error = None;
    for result in results {
        match result {
            Ok(lease) => acquired.push(lease),
            Err(error) => {
                first_error.get_or_insert(error);
            }
        }
    }
    match first_error {
        Some(error) => Err((error, acquired)),
        None => Ok(acquired),
    }
}

pub(super) fn unix_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

pub(super) fn increment(counter: &AtomicU64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        Some(value.saturating_add(1))
    });
}
