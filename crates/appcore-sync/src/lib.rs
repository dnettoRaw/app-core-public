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
    OpaqueEnvelopePolicy, MAX_OPAQUE_MESSAGE_ID_BYTES, OPAQUE_CONTENT_ENVELOPE_SCHEMA_V1,
};
pub use sync::{
    compute_events_hash, decode_sync_envelope, decode_sync_message, discover_dns_sync_peers,
    encode_sync_envelope_v1, encoded_sync_message_bytes, split_sync_payload,
    write_sync_message_json, FileReplicationLog, FileSyncCheckpointStore, FileSyncOutbox,
    FollowerSyncClient, HeartbeatMessage, HttpSyncTransport, InMemoryReplicationLog,
    InMemorySyncCheckpointStore, InMemorySyncConflictStore, InMemorySyncOutbox, LeaderElection,
    NodeRole, PeerInfo, ReplicationLog, ReplicationSnapshot, ReplicationSnapshotRecord,
    SyncCheckpointStore, SyncChunkAssembler, SyncChunkInsert, SyncChunkProgress, SyncChunkRange,
    SyncConflict, SyncConflictKind, SyncConflictResolution, SyncConflictResolutionReceipt,
    SyncConflictResolutionRequest, SyncConflictStore, SyncEnvelopeV1, SyncError, SyncMessage,
    SyncOutbox, SyncOutboxReceipt, SyncOutboxStats, SyncPayloadChunk, SyncPeerAddress,
    SyncPeerScheme, SyncPushMetrics, SyncReceiveAck, SyncReceiverState, SyncResult,
    SyncRetryPolicy, SyncStatus, SyncTransport, MAX_CHECKPOINT_FILE_BYTES,
    MAX_CHECKPOINT_PEER_ID_BYTES, MAX_CHECKPOINT_RECORDS, MAX_CONFLICT_ID_BYTES,
    MAX_CONFLICT_REASON_BYTES, MAX_OUTBOX_PAGE_BYTES, MAX_OUTBOX_PAGE_MESSAGES,
    MAX_REPLICATION_PAGE_BYTES, MAX_REPLICATION_PAGE_RECORDS, MAX_SYNC_BATCH_PAYLOAD_BYTES,
    MAX_SYNC_CHUNKS, MAX_SYNC_CHUNK_BYTES, MAX_SYNC_CHUNK_TOTAL_BYTES, MAX_SYNC_CONFLICTS,
    MAX_SYNC_REQUEST_BODY_BYTES, MAX_SYNC_TRANSFER_ID_BYTES, REPLICATION_LOG_FORMAT_V1,
    SYNC_CHECKPOINT_FORMAT_V1, SYNC_OUTBOX_FORMAT_V2, SYNC_WIRE_SCHEMA_V1,
};
