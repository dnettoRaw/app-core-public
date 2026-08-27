// =============================================================================
//        #######
//     ###       ###     F: heartbeat.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/26 08:53:09 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:48:56 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Heartbeat monitoring and worker pruning.

use crate::state::GatewayState;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::warn;

/// Spawns an owned background task that periodically prunes stale worker
/// connections until the shared Gateway state requests shutdown.
pub fn spawn_heartbeat_pruner(
    state: Arc<GatewayState>,
    interval: Duration,
    timeout: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval_ticker = tokio::time::interval(interval);
        let mut shutdown = state.subscribe_shutdown();
        loop {
            tokio::select! {
                biased;
                result = shutdown.changed() => {
                    if result.is_err() || *shutdown.borrow() {
                        break;
                    }
                    continue;
                }
                _ = interval_ticker.tick() => {}
            }
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;

            let timeout_ms = timeout.as_millis() as u64;
            let mut to_prune = Vec::new();

            {
                for (tenant_id, tenant_state) in state.tenant_entries() {
                    let tenant_state = tenant_state.read();
                    for ((inst_id, core_id), worker) in tenant_state.workers.iter() {
                        let age = now_ms.saturating_sub(worker.last_heartbeat());
                        if age > timeout_ms {
                            to_prune.push((
                                tenant_id.clone(),
                                inst_id.clone(),
                                core_id.clone(),
                                worker.generation(),
                            ));
                        }
                    }
                }
            }

            for (tenant_id, inst_id, core_id, generation) in to_prune {
                warn!(
                    "Pruning stale worker connection for tenant {} (installation: {}, core: {})",
                    tenant_id.as_str(),
                    inst_id.as_str(),
                    core_id.as_str()
                );
                let removed = state
                    .tenant_partition(&tenant_id)
                    .is_some_and(|tenant_state| {
                        let mut tenant_state = tenant_state.write();
                        tenant_state.remove_worker_if_current(&inst_id, &core_id, generation)
                    });
                if removed {
                    if let Some(coordinator) = state.ha_coordinator() {
                        let _ = coordinator
                            .remove_worker(&tenant_id, &inst_id, &core_id, generation)
                            .await;
                    }
                    state.metrics.worker_disconnected();
                }
            }
        }
    })
}
