// =============================================================================
//        #######
//     ###       ###     F: peer_projection.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/12 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/12 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Versioned payload-free peer progress, with independent presence and health.
use serde::{Deserialize, Serialize};

/// Maximum peers in one operational projection.
pub const MAX_OPERATIONAL_PEERS_V1: usize = 1_024;
/// Observation state; absence of evidence never becomes a health claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum OperationalStateV1 {
    /// No observation supplied by the owning component.
    Unknown,
    /// Component operates within its contract.
    Healthy,
    /// Reduced availability or guarantees.
    Degraded,
    /// Component is unavailable.
    Unavailable,
}
/// Queue phase reported explicitly by the delivery owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DeliveryPhaseV1 {
    /// No queued work.
    Idle,
    /// Waiting for the configured local scheduling cycle.
    Scheduled,
    /// An exchange is in flight.
    Sending,
    /// Waiting for a bounded retry.
    Retry,
    /// No usable leader is known.
    NoLeader,
    /// Automatic progression is unsafe or exhausted.
    InterventionRequired,
}
/// Scope of discovery. Known peers alone never prove complete discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DiscoveryScopeV1 {
    /// Only this installation's currently known peers are included.
    LocalKnowledge,
    /// Projection is filtered to a declared partition.
    Partition,
}
/// One opaque peer's independent transport, health and applied progress observations.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationalPeerV1 {
    /// Tenant-scoped opaque identifier, 1..=128 ASCII identifier bytes.
    pub peer_id: String,
    /// Safe role identifier, 1..=32 ASCII identifier bytes.
    pub role: String,
    /// Known in a roster; does not imply a connection or healthy service.
    pub registered: bool,
    /// Current connection observation, independent of registration and health.
    pub connected: bool,
    /// Peer health explicitly reported by its owner.
    pub health: OperationalStateV1,
    /// Last observed heartbeat wall-clock timestamp, for display only.
    pub last_heartbeat_ms: Option<u64>,
    /// Local monotonic elapsed age, never computed from a remote wall clock.
    pub heartbeat_age_ms: Option<u64>,
    /// Applied checkpoint, if independently verified.
    pub applied_checkpoint: Option<u64>,
    /// Head observed from this peer, if independently verified.
    pub observed_head: Option<u64>,
}
impl std::fmt::Debug for OperationalPeerV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OperationalPeerV1")
            .field("connected", &self.connected)
            .field("health", &self.health)
            .finish_non_exhaustive()
    }
}
impl OperationalPeerV1 {
    /// Reports lag only when both observations are coherent; unknown stays unknown.
    pub fn lag(&self) -> Option<u64> {
        self.observed_head?.checked_sub(self.applied_checkpoint?)
    }
}
/// Independent bounded operational projection for an authenticated partition.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationalProjectionV1 {
    /// Exact projection version, currently 1.
    pub format_version: u16,
    /// Scope applies only to the selected authenticated partition.
    pub discovery_scope: DiscoveryScopeV1,
    /// Local service health; Internet/Gateway loss never changes it implicitly.
    pub local_health: OperationalStateV1,
    /// Gateway reachability observation.
    pub gateway: OperationalStateV1,
    /// Persistence integrity/availability observation.
    pub persistence: OperationalStateV1,
    /// Explicit queue phase from the delivery owner.
    pub delivery_phase: DeliveryPhaseV1,
    /// Pending operations/batches; never an inferred document count.
    pub pending_batches: u64,
    /// Optional materialized object count supplied by the owner.
    pub pending_objects: Option<u64>,
    /// Monotonic elapsed age of the oldest pending batch.
    pub queue_age_ms: Option<u64>,
    /// Wall-clock timestamp of the last attempted send, for display.
    pub last_send_ms: Option<u64>,
    /// Wall-clock timestamp of the last verified acknowledgement, for display.
    pub last_ack_ms: Option<u64>,
    /// Known peer roster within the declared scope.
    pub peers: Vec<OperationalPeerV1>,
}
impl OperationalProjectionV1 {
    /// Checks bounds, unique identities and checkpoint coherence before exporting.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.format_version != 1 {
            return Err("NO MORE SUPPORTED PLEASE UPDATE");
        }
        if self.peers.len() > MAX_OPERATIONAL_PEERS_V1 {
            return Err("peer projection capacity exceeded");
        }
        let mut seen = std::collections::BTreeSet::new();
        for peer in &self.peers {
            if !identifier(&peer.peer_id, 128)
                || !identifier(&peer.role, 32)
                || !seen.insert(&peer.peer_id)
            {
                return Err("peer projection identity invalid");
            }
            if matches!((peer.applied_checkpoint, peer.observed_head), (Some(applied), Some(head)) if applied > head)
            {
                return Err("peer projection checkpoint exceeds observed head");
            }
        }
        Ok(())
    }
    /// Serializes validated payload-free evidence. Authenticate and tenant-filter
    /// source observations before constructing this projection.
    pub fn to_json(&self) -> Result<String, &'static str> {
        self.validate()?;
        serde_json::to_string(self).map_err(|_| "peer projection serialization failed")
    }
}
fn identifier(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':'))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn report() -> OperationalProjectionV1 {
        OperationalProjectionV1 {
            format_version: 1,
            discovery_scope: DiscoveryScopeV1::LocalKnowledge,
            local_health: OperationalStateV1::Healthy,
            gateway: OperationalStateV1::Unavailable,
            persistence: OperationalStateV1::Healthy,
            delivery_phase: DeliveryPhaseV1::Scheduled,
            pending_batches: 2,
            pending_objects: None,
            queue_age_ms: Some(20),
            last_send_ms: None,
            last_ack_ms: None,
            peers: vec![OperationalPeerV1 {
                peer_id: "peer-a".into(),
                role: "leader".into(),
                registered: true,
                connected: false,
                health: OperationalStateV1::Unknown,
                last_heartbeat_ms: Some(100),
                heartbeat_age_ms: Some(20),
                applied_checkpoint: Some(3),
                observed_head: Some(5),
            }],
        }
    }
    #[test]
    fn presence_gateway_and_service_health_remain_independent() {
        let projection = report();
        assert_eq!(projection.peers[0].lag(), Some(2));
        assert_eq!(projection.local_health, OperationalStateV1::Healthy);
        assert!(!projection.peers[0].connected);
        let mut clock_jump = projection.clone();
        clock_jump.peers[0].last_heartbeat_ms = Some(u64::MAX);
        assert_eq!(clock_jump.peers[0].heartbeat_age_ms, Some(20));
        assert!(projection.to_json().unwrap().contains("scheduled"));
    }
    #[test]
    fn rejects_future_versions_duplicate_peers_and_impossible_checkpoints() {
        let mut projection = report();
        projection.format_version = 2;
        assert_eq!(
            projection.validate(),
            Err("NO MORE SUPPORTED PLEASE UPDATE")
        );
        let mut projection = report();
        projection.peers.push(projection.peers[0].clone());
        assert!(projection.validate().is_err());
        let mut projection = report();
        projection.peers[0].applied_checkpoint = Some(6);
        assert!(projection.validate().is_err());
    }
}
