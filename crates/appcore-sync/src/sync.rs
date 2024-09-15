// =============================================================================
//        #######
//     ###       ###     F: sync.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/31 13:38:42 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Minimal sync contracts and local implementations.
mod checkpoint;
mod client;
mod codec;
mod discovery;
mod error;
mod log;
mod outbox;
mod persistence;
mod receiver;
mod retry;
mod snapshot;
mod transport;
mod types;
mod wire;

pub use checkpoint::{
    FileSyncCheckpointStore, InMemorySyncCheckpointStore, SyncCheckpointStore,
    SYNC_CHECKPOINT_FORMAT_V1,
};
pub use client::FollowerSyncClient;
pub use discovery::{discover_dns_sync_peers, SyncPeerAddress, SyncPeerScheme};
pub use error::{SyncError, SyncResult};
pub use log::{
    FileReplicationLog, InMemoryReplicationLog, ReplicationLog, REPLICATION_LOG_FORMAT_V1,
};
pub use outbox::{FileSyncOutbox, InMemorySyncOutbox, SyncOutbox, SYNC_OUTBOX_FORMAT_V1};
pub use receiver::{SyncReceiveAck, SyncReceiverState};
pub use retry::{SyncPushMetrics, SyncRetryPolicy};
pub use snapshot::{ReplicationSnapshot, ReplicationSnapshotRecord, SYNC_SNAPSHOT_FORMAT_V1};
pub use transport::{decode_sync_message, HttpSyncTransport, SyncTransport};
pub use types::{
    compute_events_hash, HeartbeatMessage, LeaderElection, NodeRole, PeerInfo, SyncMessage,
    SyncStatus,
};
pub use wire::{
    decode_sync_envelope, encode_sync_envelope_v1, SyncEnvelopeV1, SYNC_WIRE_SCHEMA_V1,
};

#[cfg(test)]
pub(crate) use transport::read_http_request_body;

#[cfg(test)]
mod sync_tests;
