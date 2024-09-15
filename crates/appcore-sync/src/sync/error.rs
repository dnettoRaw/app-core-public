// =============================================================================
//        #######
//     ###       ###     F: error.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/02 13:08:16 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Sync-local typed errors.

pub(crate) const UPDATE_REQUIRED_MESSAGE: &str = "NO MORE SUPPORTED PLEASE UPDATE";

/// Sync-local result type.
pub type SyncResult<T> = Result<T, SyncError>;

/// Sync-local typed errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncError {
    /// The remote peer does not share the local application sync identity.
    IncompatiblePeer,
    /// A transport operation failed for the supplied reason.
    TransportFailed(String),
    /// A transport operation exceeded its configured deadline.
    TransportTimeout(String),
    /// The remote HTTP endpoint returned a non-success status.
    HttpStatus(u16),
    /// The remote endpoint closed the connection without a response.
    EmptyHttpResponse,
    /// A sync request did not contain a body.
    EmptyRequestBody,
    /// An outbound request exceeded the configured byte limit.
    RequestBodyTooLarge {
        /// Actual encoded body size in bytes.
        size: usize,
        /// Maximum accepted body size in bytes.
        max: usize,
    },
    /// An inbound response exceeded the configured byte limit.
    ResponseTooLarge {
        /// Maximum accepted response size in bytes.
        max: usize,
    },
    /// A batch contained more events than the receiver accepts.
    TooManyEvents {
        /// Number of events found in the batch.
        count: usize,
        /// Maximum accepted event count.
        max: usize,
    },
    /// A sequence does not continue or validly overlap the checkpoint.
    InvalidSequence(u64),
    /// A sequence already exists with different payload bytes.
    SequenceConflict(u64),
    /// The selected replication log does not implement snapshots.
    SnapshotUnsupported,
    /// A snapshot failed integrity or structural validation.
    InvalidSnapshot(&'static str),
    /// A peer identifier is empty or contains unsupported characters.
    InvalidPeerId,
    /// A peer seed cannot be parsed as a supported host and port.
    InvalidPeerAddress,
    /// A peer seed uses a transport scheme unsupported by sync.
    UnsupportedPeerScheme,
    /// A DNS seed could not be resolved.
    DnsResolutionFailed(String),
    /// TLS setup or negotiation failed.
    TlsFailed(String),
    /// A replication-log read started beyond the current log length.
    LogIndexOutOfBounds {
        /// Requested zero-based offset.
        index: usize,
        /// Current log length.
        len: usize,
    },
    /// A synchronization primitive was poisoned.
    LockPoisoned(&'static str),
    /// A wire message violated a sync invariant.
    InvalidSyncMessage(&'static str),
    /// An event payload is not valid hexadecimal data.
    InvalidEventHex,
    /// A durable replication-log record is malformed.
    CorruptReplicationLog {
        /// One-based line containing the invalid record.
        line: usize,
        /// Stable description of the violated record format.
        reason: &'static str,
    },
    /// A durable outbox record is malformed.
    CorruptOutbox {
        /// One-based line containing the invalid record.
        line: usize,
    },
    /// Leader-election state could not be advanced.
    ElectionFailed(String),
    /// Replication persistence or application failed.
    ReplicationFailed(String),
    /// A requested peer is absent from the current directory.
    PeerNotFound(String),
}
