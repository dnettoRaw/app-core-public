// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/29 20:47:35 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 00:04:12 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Sync contracts for roles, peer metadata, transport, election, and replication.

#![deny(missing_docs)]

pub mod sync;

pub use appcore_distributed_contracts::{
    OpaqueContentEnvelopeV1, OpaqueEnvelopeDecision, OpaqueEnvelopeDeduplicator,
    OpaqueEnvelopePolicy, OPAQUE_CONTENT_ENVELOPE_SCHEMA_V1,
};
pub use sync::{
    compute_events_hash, decode_sync_envelope, decode_sync_message, discover_dns_sync_peers,
    encode_sync_envelope_v1, FileReplicationLog, FileSyncCheckpointStore, FileSyncOutbox,
    FollowerSyncClient, HeartbeatMessage, HttpSyncTransport, InMemoryReplicationLog,
    InMemorySyncCheckpointStore, InMemorySyncOutbox, LeaderElection, NodeRole, PeerInfo,
    ReplicationLog, ReplicationSnapshot, ReplicationSnapshotRecord, SyncCheckpointStore,
    SyncEnvelopeV1, SyncError, SyncMessage, SyncOutbox, SyncOutboxReceipt, SyncOutboxStats,
    SyncPeerAddress, SyncPeerScheme, SyncPushMetrics, SyncReceiveAck, SyncReceiverState,
    SyncResult, SyncRetryPolicy, SyncStatus, SyncTransport, MAX_OUTBOX_PAGE_BYTES,
    MAX_OUTBOX_PAGE_MESSAGES, REPLICATION_LOG_FORMAT_V1, SYNC_CHECKPOINT_FORMAT_V1,
    SYNC_OUTBOX_FORMAT_V2, SYNC_WIRE_SCHEMA_V1,
};
