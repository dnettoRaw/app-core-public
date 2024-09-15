// =============================================================================
//        #######
//     ###       ###     F: wire.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/21 19:22:40 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Versioned wire envelopes for replication messages.

use crate::sync::error::{SyncError, SyncResult, UPDATE_REQUIRED_MESSAGE};
use crate::sync::types::SyncMessage;
use appcore_core::{CoreCompatibilityPolicy, CoreIdentity};

/// Schema identifier for the first identity-aware sync wire envelope.
pub const SYNC_WIRE_SCHEMA_V1: &str = "appcore.sync.v1";

/// Versioned sync envelope carrying the complete distributed source identity.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SyncEnvelopeV1 {
    /// Stable wire schema identifier.
    pub schema: String,
    /// Identity of the Core that produced the replication batch.
    pub source_identity: CoreIdentity,
    /// Replication message protected by its existing event hash chain.
    pub message: SyncMessage,
}

impl SyncEnvelopeV1 {
    /// Creates a v1 envelope after checking the message/source node binding.
    pub fn new(source_identity: CoreIdentity, message: SyncMessage) -> SyncResult<Self> {
        if source_identity.runtime.node_id != message.source_node_id {
            return Err(SyncError::InvalidSyncMessage(
                "source identity does not match message node",
            ));
        }
        Ok(Self {
            schema: SYNC_WIRE_SCHEMA_V1.to_string(),
            source_identity,
            message,
        })
    }

    /// Verifies schema, source binding and compatibility with the receiver.
    pub fn validate_for(&self, local_identity: &CoreIdentity) -> SyncResult<()> {
        if self.schema != SYNC_WIRE_SCHEMA_V1 {
            return Err(SyncError::InvalidSyncMessage(
                "unsupported sync wire schema",
            ));
        }
        if self.source_identity.runtime.node_id != self.message.source_node_id {
            return Err(SyncError::InvalidSyncMessage(
                "source identity does not match message node",
            ));
        }
        let policy = CoreCompatibilityPolicy {
            require_same_cluster: true,
            required_capability: None,
        };
        local_identity
            .ensure_compatible(&self.source_identity, &policy, &[])
            .map_err(|_| SyncError::IncompatiblePeer)
    }
}

/// Encodes an identity-aware v1 envelope as JSON.
pub fn encode_sync_envelope_v1(
    source_identity: &CoreIdentity,
    message: &SyncMessage,
) -> SyncResult<String> {
    let envelope = SyncEnvelopeV1::new(source_identity.clone(), message.clone())?;
    serde_json::to_string(&envelope)
        .map_err(|_| SyncError::InvalidSyncMessage("sync wire serialization failed"))
}

/// Decodes the identity-aware v1 JSON envelope.
pub fn decode_sync_envelope(input: &str) -> SyncResult<SyncEnvelopeV1> {
    if input.is_empty() {
        return Err(SyncError::EmptyRequestBody);
    }
    if !input.trim_start().starts_with('{') {
        return Err(SyncError::InvalidSyncMessage(UPDATE_REQUIRED_MESSAGE));
    }
    let envelope = serde_json::from_str::<SyncEnvelopeV1>(input)
        .map_err(|_| SyncError::InvalidSyncMessage("invalid sync wire envelope"))?;
    if envelope.schema != SYNC_WIRE_SCHEMA_V1 {
        return Err(SyncError::InvalidSyncMessage(UPDATE_REQUIRED_MESSAGE));
    }
    if envelope.source_identity.runtime.node_id != envelope.message.source_node_id {
        return Err(SyncError::InvalidSyncMessage(
            "source identity does not match message node",
        ));
    }
    Ok(envelope)
}

#[cfg(test)]
mod tests {
    use super::*;
    use appcore_core::{
        AppFamily, AppId, ClusterId, CoreId, CoreKind, InstanceId, NodeId, ProtocolVersion,
        RuntimeContractVersion, RuntimeIdentity, SyncGroup, TenantId,
    };

    fn identity(tenant: &str, node: &str) -> CoreIdentity {
        CoreIdentity {
            tenant_id: TenantId::new(tenant).unwrap(),
            cluster_id: ClusterId::new("cluster-a").unwrap(),
            core_id: CoreId::new(format!("core-{node}")).unwrap(),
            instance_id: InstanceId::new(format!("instance-{node}")).unwrap(),
            kind: CoreKind::new("replica").unwrap(),
            protocol_version: ProtocolVersion::new(1),
            runtime: RuntimeIdentity {
                app_id: AppId::new("app-a").unwrap(),
                app_family: AppFamily::new("family-a").unwrap(),
                sync_group: SyncGroup::new("cluster-a").unwrap(),
                runtime_contract: RuntimeContractVersion::new(1),
                node_id: NodeId::new(node).unwrap(),
            },
        }
    }

    fn message() -> SyncMessage {
        SyncMessage {
            batch_id: "batch-1".to_string(),
            source_node_id: NodeId::new("node-a").unwrap(),
            sequence_start: 1,
            sequence_end: 1,
            event_count: 1,
            events_hash: "hash".to_string(),
            created_at_ms: 10,
            previous_batch_hash: None,
            events: vec![b"event".to_vec()],
        }
    }

    #[test]
    fn v1_encoding_matches_golden_fixture() {
        let encoded = encode_sync_envelope_v1(&identity("tenant-a", "node-a"), &message())
            .expect("v1 envelope");

        assert_eq!(encoded, include_str!("fixtures/sync-wire-v1.json").trim());
        assert!(decode_sync_envelope(&encoded).is_ok());
    }

    #[test]
    fn v1_rejects_source_node_mismatch() {
        assert!(matches!(
            SyncEnvelopeV1::new(identity("tenant-a", "node-b"), message()),
            Err(SyncError::InvalidSyncMessage(_))
        ));
    }

    #[test]
    fn v1_rejects_incompatible_tenant() {
        let envelope = SyncEnvelopeV1::new(identity("tenant-a", "node-a"), message()).unwrap();

        assert_eq!(
            envelope.validate_for(&identity("tenant-b", "node-b")),
            Err(SyncError::IncompatiblePeer)
        );
    }

    #[test]
    fn decoder_rejects_unversioned_wire_with_update_wall() {
        assert_eq!(
            decode_sync_envelope("batch-1\nnode-a\n1\n1\n"),
            Err(SyncError::InvalidSyncMessage(UPDATE_REQUIRED_MESSAGE))
        );
    }
}
