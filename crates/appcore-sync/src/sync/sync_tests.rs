// =============================================================================
//        #######
//     ###       ###     F: sync_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:42:05 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 13:24:05 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::{
    decode_sync_message, read_http_request_body, FileReplicationLog, FileSyncCheckpointStore,
    FileSyncOutbox, FollowerSyncClient, HeartbeatMessage, HttpSyncTransport,
    InMemoryReplicationLog, InMemorySyncCheckpointStore, InMemorySyncOutbox, LeaderElection,
    NodeRole, PeerInfo, ReplicationLog, SyncCheckpointStore, SyncError, SyncMessage, SyncOutbox,
    SyncOutboxReceipt, SyncReceiverState, SyncTransport, MAX_OUTBOX_PAGE_BYTES,
    MAX_OUTBOX_PAGE_MESSAGES, REPLICATION_LOG_FORMAT_V1, SYNC_CHECKPOINT_FORMAT_V1,
    SYNC_OUTBOX_FORMAT_V2,
};
use appcore_core::{
    AppFamily, AppId, ClusterId, CoreId, CoreIdentity, CoreKind, InstanceId, NodeId,
    ProtocolVersion, RuntimeContractVersion, RuntimeIdentity, SyncGroup, TenantId,
};
use appcore_ops::Heartbeat;
use parking_lot::Mutex;
use std::io::Write;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

const TEST_HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const TEST_HASH_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn test_core_identity(node: &str) -> CoreIdentity {
    CoreIdentity {
        tenant_id: TenantId::new("tenant-a").unwrap(),
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

struct MockElection {
    role: NodeRole,
}

impl LeaderElection for MockElection {
    fn current_role(&self) -> NodeRole {
        self.role
    }

    fn vote(&mut self, _candidate: &NodeId) -> super::SyncResult<()> {
        self.role = NodeRole::Follower;
        Ok(())
    }

    fn become_leader(&mut self) -> super::SyncResult<()> {
        self.role = NodeRole::Leader;
        Ok(())
    }
}

struct MockLog {
    records: Vec<Vec<u8>>,
}

impl ReplicationLog for MockLog {
    fn append(&mut self, record: Vec<u8>) -> super::SyncResult<usize> {
        self.records.push(record);
        Ok(self.records.len())
    }

    fn append_with_sequence(
        &mut self,
        record: Vec<u8>,
        _sequence: u64,
    ) -> super::SyncResult<usize> {
        self.records.push(record);
        Ok(self.records.len())
    }

    fn events_since(&self, index: usize) -> super::SyncResult<Vec<Vec<u8>>> {
        if index > self.records.len() {
            return Err(super::SyncError::ReplicationFailed("bad index".to_string()));
        }
        Ok(self.records[index..].to_vec())
    }

    fn last_index(&self) -> super::SyncResult<usize> {
        Ok(self.records.len())
    }

    fn len(&self) -> super::SyncResult<usize> {
        Ok(self.records.len())
    }

    fn is_empty(&self) -> super::SyncResult<bool> {
        Ok(self.records.is_empty())
    }
}

struct MockTransport {
    sent: usize,
}

struct FailOnceCheckpointStore {
    failed: AtomicBool,
    inner: InMemorySyncCheckpointStore,
}

impl FailOnceCheckpointStore {
    fn new() -> Self {
        Self {
            failed: AtomicBool::new(false),
            inner: InMemorySyncCheckpointStore::new(),
        }
    }
}

impl SyncCheckpointStore for FailOnceCheckpointStore {
    fn get_checkpoint(&self, peer_id: &str) -> super::SyncResult<Option<(u64, String)>> {
        self.inner.get_checkpoint(peer_id)
    }

    fn set_checkpoint(&self, peer_id: &str, sequence: u64, hash: &str) -> super::SyncResult<()> {
        if !self.failed.swap(true, Ordering::SeqCst) {
            return Err(super::SyncError::ReplicationFailed(
                "injected checkpoint failure".to_string(),
            ));
        }
        self.inner.set_checkpoint(peer_id, sequence, hash)
    }
}

impl SyncTransport for MockTransport {
    fn send_heartbeat(&mut self, _heartbeat: HeartbeatMessage) -> super::SyncResult<()> {
        self.sent += 1;
        Ok(())
    }

    fn send_payload(&mut self, _peer: &PeerInfo, _payload: Vec<u8>) -> super::SyncResult<()> {
        self.sent += 1;
        Ok(())
    }
}

fn identity(app_id: &str, node_id: &str) -> RuntimeIdentity {
    RuntimeIdentity {
        app_id: AppId::new(app_id.to_string()).unwrap(),
        app_family: AppFamily::new("family".to_string()).unwrap(),
        sync_group: SyncGroup::new("dev".to_string()).unwrap(),
        runtime_contract: RuntimeContractVersion::new(1),
        node_id: NodeId::new(node_id.to_string()).unwrap(),
    }
}

#[test]
fn peer_compatibility_ok() {
    let local = identity("app-a", "n1");
    let peer = PeerInfo {
        identity: identity("app-a", "n2"),
        role: NodeRole::Follower,
        last_seen_ms: 1,
    };
    assert!(peer.ensure_compatible_with(&local).is_ok());
}

#[test]
fn peer_compatibility_rejects_mismatch() {
    let local = identity("app-a", "n1");
    let peer = PeerInfo {
        identity: identity("app-b", "n2"),
        role: NodeRole::Follower,
        last_seen_ms: 1,
    };
    assert_eq!(
        peer.ensure_compatible_with(&local),
        Err(super::SyncError::IncompatiblePeer)
    );
}

#[test]
fn heartbeat_message_from_ops_heartbeat() {
    let message = HeartbeatMessage::from(Heartbeat {
        node_id: NodeId::new("n1".to_string()).unwrap(),
        timestamp_ms: 10,
    });
    assert_eq!(message.node_id, NodeId::new("n1".to_string()).unwrap());
}

#[test]
fn leader_election_mock() {
    let mut election = MockElection {
        role: NodeRole::Candidate,
    };
    assert!(election
        .vote(&NodeId::new("n2".to_string()).unwrap())
        .is_ok());
    assert_eq!(election.current_role(), NodeRole::Follower);
    assert!(election.become_leader().is_ok());
    assert_eq!(election.current_role(), NodeRole::Leader);
}

#[test]
fn replication_log_mock() {
    let mut log = MockLog {
        records: Vec::new(),
    };
    let index = log.append(vec![1]);
    assert!(index.is_ok());
    assert_eq!(index.unwrap_or_default(), 1);
    assert_eq!(log.len(), Ok(1));
    let events = log.events_since(0);
    assert!(events.is_ok());
    let events = match events {
        Ok(events) => events,
        Err(_) => return,
    };
    assert_eq!(events.len(), 1);
}

#[test]
fn sync_transport_mock() {
    let mut transport = MockTransport { sent: 0 };
    let peer = PeerInfo {
        identity: identity("app-a", "n2"),
        role: NodeRole::Follower,
        last_seen_ms: 1,
    };
    assert!(transport
        .send_heartbeat(HeartbeatMessage {
            node_id: NodeId::new("n1".to_string()).unwrap(),
            timestamp_ms: 10
        })
        .is_ok());
    assert!(transport.send_payload(&peer, vec![1, 2]).is_ok());
    assert_eq!(transport.sent, 2);
}

#[test]
fn typed_sync_error_exists() {
    let err = super::SyncError::PeerNotFound("n2".to_string());
    assert_eq!(err, super::SyncError::PeerNotFound("n2".to_string()));
}

#[test]
fn leader_pushes_three_events_and_follower_receives_three() {
    let follower_log = Arc::new(Mutex::new(InMemoryReplicationLog::new()));
    let follower_log_for_server = Arc::clone(&follower_log);
    let listener = TcpListener::bind("127.0.0.1:0");
    assert!(listener.is_ok());
    let listener = match listener {
        Ok(listener) => listener,
        Err(_) => return,
    };
    let port = match listener.local_addr() {
        Ok(addr) => addr.port(),
        Err(_) => return,
    };

    let server = thread::spawn(move || {
        let accepted = listener.accept();
        assert!(accepted.is_ok());
        let (mut stream, _) = match accepted {
            Ok(value) => value,
            Err(_) => return,
        };
        let body = read_http_request_body(&mut stream);
        assert!(body.is_ok());
        let body = match body {
            Ok(body) => body,
            Err(_) => return,
        };
        let parsed = decode_sync_message(&body);
        assert!(parsed.is_ok());
        let message = match parsed {
            Ok(message) => message,
            Err(_) => return,
        };
        let mut guard = follower_log_for_server.lock();
        for (index, event) in message.events.into_iter().enumerate() {
            let sequence = message.sequence_start + index as u64;
            let _ = guard.append_with_sequence(event, sequence);
        }
        let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK");
    });

    let mut leader_log = InMemoryReplicationLog::new();
    assert!(leader_log.append(b"evt-1".to_vec()).is_ok());
    assert!(leader_log.append(b"evt-2".to_vec()).is_ok());
    assert!(leader_log.append(b"evt-3".to_vec()).is_ok());
    let events = leader_log.events_since(0);
    assert!(events.is_ok());
    let events = match events {
        Ok(events) => events,
        Err(_) => return,
    };
    let message = SyncMessage::new_simple(NodeId::new("leader-a".to_string()).unwrap(), 3, events);
    let client = FollowerSyncClient::new(
        HttpSyncTransport::new("127.0.0.1", port)
            .with_source_identity(test_core_identity("leader-a")),
    );
    assert!(client.push_events(&message).is_ok());
    assert!(server.join().is_ok());
    let guard = follower_log.lock();
    assert_eq!(guard.len(), Ok(3));
}

#[test]
fn push_retry_exhausted_when_follower_unavailable() {
    let port = 9; // discard port, expected closed locally
    let message = SyncMessage::new_simple(
        NodeId::new("leader-a".to_string()).unwrap(),
        1,
        vec![b"x".to_vec()],
    );
    let policy = super::SyncRetryPolicy {
        max_attempts: 2,
        backoff_ms: 0,
        max_queue_len: 8,
    };
    let client = FollowerSyncClient::new(
        HttpSyncTransport::new("127.0.0.1", port)
            .with_source_identity(test_core_identity("leader-a")),
    )
    .with_retry_policy(policy);

    let result = client.push_events(&message);
    assert!(result.is_err());
    let metrics = client.metrics();
    assert_eq!(metrics.push_attempt, 2);
    assert_eq!(metrics.push_success, 0);
    assert_eq!(metrics.push_failed, 1);
    assert_eq!(metrics.push_dropped, 0);
    assert_eq!(client.pending_len(), 1);
    assert_eq!(client.outbox_stats().unwrap().total_attempts, Some(2));
    assert_eq!(
        client.pending_page(1, 1_024 * 1_024).unwrap(),
        vec![message]
    );
}

#[test]
fn push_recovery_flushes_pending_after_server_returns() {
    let reserved = TcpListener::bind("127.0.0.1:0");
    assert!(reserved.is_ok());
    let reserved = match reserved {
        Ok(listener) => listener,
        Err(_) => return,
    };
    let port = match reserved.local_addr() {
        Ok(addr) => addr.port(),
        Err(_) => return,
    };
    drop(reserved);

    let policy = super::SyncRetryPolicy {
        max_attempts: 1,
        backoff_ms: 0,
        max_queue_len: 8,
    };
    let client = FollowerSyncClient::new(
        HttpSyncTransport::new("127.0.0.1", port)
            .with_source_identity(test_core_identity("leader-a")),
    )
    .with_retry_policy(policy);
    let message = SyncMessage::new_simple(
        NodeId::new("leader-a".to_string()).unwrap(),
        2,
        vec![b"a".to_vec()],
    );

    let first = client.push_events(&message);
    assert!(first.is_err());
    assert_eq!(client.pending_len(), 1);

    let listener = TcpListener::bind(("127.0.0.1", port));
    assert!(listener.is_ok());
    let listener = match listener {
        Ok(listener) => listener,
        Err(_) => return,
    };
    let server = thread::spawn(move || {
        let accepted = listener.accept();
        assert!(accepted.is_ok());
        let (mut stream, _) = match accepted {
            Ok(value) => value,
            Err(_) => return,
        };
        let body = read_http_request_body(&mut stream);
        assert!(body.is_ok());
        let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK");
    });

    let second = client.flush_pending_with_progress();
    assert_eq!(second, Ok(Some(message)));
    assert!(server.join().is_ok());
    assert_eq!(client.pending_len(), 0);
    let metrics = client.metrics();
    assert_eq!(metrics.push_success, 1);
    assert_eq!(metrics.push_failed, 1);
}

#[test]
fn queue_full_drops_new_message() {
    let port = 9;
    let policy = super::SyncRetryPolicy {
        max_attempts: 1,
        backoff_ms: 0,
        max_queue_len: 1,
    };
    let client = FollowerSyncClient::new(
        HttpSyncTransport::new("127.0.0.1", port)
            .with_source_identity(test_core_identity("leader-a")),
    )
    .with_retry_policy(policy);
    let message1 = SyncMessage::new_simple(
        NodeId::new("leader-a".to_string()).unwrap(),
        1,
        vec![b"a".to_vec()],
    );
    let message2 = SyncMessage::new_simple(
        NodeId::new("leader-a".to_string()).unwrap(),
        2,
        vec![b"b".to_vec()],
    );

    assert!(client.push_events(&message1).is_err());
    assert_eq!(client.pending_len(), 1);
    let second = client.push_events(&message2);
    assert!(second.is_err());
    let metrics = client.metrics();
    assert_eq!(metrics.push_dropped, 1);
    assert_eq!(client.pending_len(), 1);
}

#[test]
fn in_memory_outbox_pages_readiness_stats_and_partial_receipts_are_bounded() {
    let outbox = InMemorySyncOutbox::new();
    let messages = (1..=3)
        .map(|sequence| {
            SyncMessage::new_simple(
                NodeId::new("leader-page".to_string()).unwrap(),
                sequence,
                vec![vec![sequence as u8; sequence as usize * 16]],
            )
        })
        .collect::<Vec<_>>();
    for message in &messages {
        assert_eq!(outbox.try_enqueue(message.clone(), 8), Ok(true));
    }
    let first_bytes = serde_json::to_vec(&messages[0]).unwrap().len();
    let second_bytes = serde_json::to_vec(&messages[1]).unwrap().len();
    assert!(outbox.peek(2, first_bytes - 1).unwrap().is_empty());
    assert_eq!(
        outbox.peek(3, first_bytes).unwrap(),
        vec![messages[0].clone()]
    );
    assert_eq!(
        outbox.peek(3, first_bytes + second_bytes).unwrap(),
        messages[..2]
    );
    assert!(outbox.peek(MAX_OUTBOX_PAGE_MESSAGES + 1, 1).is_err());
    assert!(outbox.peek(1, MAX_OUTBOX_PAGE_BYTES + 1).is_err());

    assert_eq!(outbox.mark_attempt(&messages[0].batch_id, 500), Ok(1));
    assert!(outbox.next_ready(499, 3, 1_024 * 1_024).unwrap().is_empty());
    assert_eq!(outbox.next_ready(500, 3, 1_024 * 1_024).unwrap(), messages);
    let stats = outbox.stats().unwrap();
    assert_eq!(stats.pending_messages, 3);
    assert_eq!(stats.attempted_messages, Some(1));
    assert_eq!(stats.total_attempts, Some(1));
    assert_eq!(stats.next_ready_at_ms, Some(500));
    assert_eq!(
        stats.pending_bytes,
        Some(
            messages
                .iter()
                .map(|message| serde_json::to_vec(message).unwrap().len())
                .sum()
        )
    );

    let wrong = SyncOutboxReceipt::new(vec![messages[1].batch_id.clone()]).unwrap();
    assert!(outbox.acknowledge_receipt(&wrong).is_err());
    assert_eq!(outbox.len(), Ok(3));
    let partial = SyncOutboxReceipt::new(
        messages[..2]
            .iter()
            .map(|message| message.batch_id.clone())
            .collect(),
    )
    .unwrap();
    assert_eq!(outbox.acknowledge_receipt(&partial), Ok(2));
    assert_eq!(outbox.front(), Ok(Some(messages[2].clone())));
}

#[test]
fn pre_extension_outbox_defaults_are_single_item_or_explicitly_unsupported() {
    struct CompatibleOutbox(SyncMessage);

    impl SyncOutbox for CompatibleOutbox {
        fn try_enqueue(&self, _message: SyncMessage, _max_len: usize) -> super::SyncResult<bool> {
            Ok(false)
        }

        fn front(&self) -> super::SyncResult<Option<SyncMessage>> {
            Ok(Some(self.0.clone()))
        }

        fn acknowledge_front(&self, _batch_id: &str) -> super::SyncResult<()> {
            Ok(())
        }

        fn messages(&self) -> super::SyncResult<Vec<SyncMessage>> {
            Ok(vec![self.0.clone()])
        }

        fn len(&self) -> super::SyncResult<usize> {
            Ok(1)
        }
    }

    let message = SyncMessage::new_simple(
        NodeId::new("v1-node".to_string()).unwrap(),
        1,
        vec![b"v1-event".to_vec()],
    );
    let encoded_bytes = serde_json::to_vec(&message).unwrap().len();
    let outbox = CompatibleOutbox(message.clone());
    assert_eq!(
        outbox.peek(8, encoded_bytes).unwrap(),
        vec![message.clone()]
    );
    assert_eq!(
        outbox.next_ready(0, 8, encoded_bytes).unwrap(),
        vec![message]
    );
    assert_eq!(outbox.stats().unwrap().pending_bytes, None);
    assert_eq!(
        outbox.mark_attempt("batch-v1-node-1", 1),
        Err(SyncError::OutboxOperationUnsupported("mark_attempt"))
    );
    let receipt = SyncOutboxReceipt::new(vec!["a".to_string(), "b".to_string()]).unwrap();
    assert_eq!(
        outbox.acknowledge_receipt(&receipt),
        Err(SyncError::OutboxOperationUnsupported(
            "multi-message receipt"
        ))
    );
}

#[test]
fn receiver_deduplicates_by_sequence() {
    let checkpoint = Arc::new(InMemorySyncCheckpointStore::new());
    checkpoint.set_last_sequence("leader-a", 9).unwrap();
    let log: Arc<Mutex<Box<dyn ReplicationLog + Send>>> =
        Arc::new(Mutex::new(Box::new(InMemoryReplicationLog::new())));
    let state = SyncReceiverState::new(Arc::clone(&log), checkpoint);
    let first = SyncMessage::new(
        "batch-seq-dup-1".to_string(),
        NodeId::new("leader-a".to_string()).unwrap(),
        10,
        11,
        0,
        None,
        vec![b"a".to_vec(), b"b".to_vec()],
    );
    let ack1 = state.apply_sync_message(&first);
    assert!(ack1.is_ok());
    let ack1 = match ack1 {
        Ok(ack) => ack,
        Err(_) => return,
    };
    assert_eq!(ack1.received, 2);
    assert_eq!(ack1.skipped, 0);
    assert_eq!(ack1.last_sequence, 11);

    let same = SyncMessage::new(
        "batch-seq-dup-2".to_string(),
        NodeId::new("leader-a".to_string()).unwrap(),
        10,
        11,
        0,
        None,
        vec![b"a".to_vec(), b"b".to_vec()],
    );
    let ack2 = state.apply_sync_message(&same);
    assert!(ack2.is_ok());
    let ack2 = match ack2 {
        Ok(ack) => ack,
        Err(_) => return,
    };
    assert_eq!(ack2.received, 0);
    assert_eq!(ack2.skipped, 2);
    assert_eq!(ack2.last_sequence, 11);

    let newer = SyncMessage::new(
        "batch-seq-dup-3".to_string(),
        NodeId::new("leader-a".to_string()).unwrap(),
        12,
        12,
        0,
        Some(first.events_hash.clone()),
        vec![b"c".to_vec()],
    );
    let ack3 = state.apply_sync_message(&newer);
    assert!(ack3.is_ok());
    let ack3 = match ack3 {
        Ok(ack) => ack,
        Err(_) => return,
    };
    assert_eq!(ack3.received, 1);
    assert_eq!(ack3.skipped, 0);
    assert_eq!(ack3.last_sequence, 12);
}

#[test]
fn receiver_rejects_excessive_event_count() {
    let checkpoint = Arc::new(InMemorySyncCheckpointStore::new());
    let log: Arc<Mutex<Box<dyn ReplicationLog + Send>>> =
        Arc::new(Mutex::new(Box::new(InMemoryReplicationLog::new())));
    let state = SyncReceiverState::new(log, checkpoint);
    let message = SyncMessage::new_simple(
        NodeId::new("leader-a".to_string()).unwrap(),
        1,
        vec![Vec::new(); 10_001],
    );
    assert_eq!(
        state.apply_sync_message(&message),
        Err(super::SyncError::TooManyEvents {
            count: 10_001,
            max: 10_000
        })
    );
}

#[test]
fn receiver_skips_replay_and_out_of_order_sequence() {
    let checkpoint = Arc::new(InMemorySyncCheckpointStore::new());
    checkpoint.set_last_sequence("leader-a", 4).unwrap();
    let log: Arc<Mutex<Box<dyn ReplicationLog + Send>>> =
        Arc::new(Mutex::new(Box::new(InMemoryReplicationLog::new())));
    let state = SyncReceiverState::new(Arc::clone(&log), checkpoint);
    let accepted = SyncMessage::new(
        "batch-replay-1".to_string(),
        NodeId::new("leader-a".to_string()).unwrap(),
        5,
        5,
        0,
        None,
        vec![b"a".to_vec()],
    );
    let replay = SyncMessage::new(
        "batch-replay-2".to_string(),
        NodeId::new("leader-a".to_string()).unwrap(),
        5,
        5,
        0,
        None,
        vec![b"a".to_vec()],
    );
    let older = SyncMessage::new(
        "batch-replay-3".to_string(),
        NodeId::new("leader-a".to_string()).unwrap(),
        4,
        4,
        0,
        None,
        vec![b"old".to_vec()],
    );
    let newer = SyncMessage::new(
        "batch-replay-4".to_string(),
        NodeId::new("leader-a".to_string()).unwrap(),
        6,
        6,
        0,
        Some(accepted.events_hash.clone()),
        vec![b"b".to_vec()],
    );

    assert_eq!(
        state.apply_sync_message(&accepted).map(|ack| ack.received),
        Ok(1)
    );
    assert_eq!(
        state.apply_sync_message(&replay).map(|ack| ack.skipped),
        Ok(1)
    );
    assert_eq!(
        state.apply_sync_message(&older).map(|ack| ack.skipped),
        Ok(1)
    );
    assert_eq!(
        state.apply_sync_message(&newer).map(|ack| ack.received),
        Ok(1)
    );
    assert_eq!(log.lock().len(), Ok(2));
}

fn unique_path(name: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "appcore-sync-{name}-{}-{ts}.txt",
        std::process::id()
    ))
}

#[test]
fn file_checkpoint_store_saves_and_reloads() {
    let path = unique_path("checkpoint-store");
    let store = FileSyncCheckpointStore::new(path.clone());
    assert!(store.is_ok());
    let store = match store {
        Ok(store) => store,
        Err(_) => return,
    };
    assert!(store.set_last_sequence("peer-a", 42).is_ok());

    let reloaded = FileSyncCheckpointStore::new(path.clone());
    assert!(reloaded.is_ok());
    let reloaded = match reloaded {
        Ok(store) => store,
        Err(_) => return,
    };
    let sequence = reloaded.get_last_sequence("peer-a");
    assert!(sequence.is_ok());
    assert_eq!(sequence.unwrap_or(0), 42);
    let _ = std::fs::remove_file(path);
}

#[test]
fn concurrent_checkpoint_instances_do_not_lose_distinct_peers() {
    let path = unique_path("checkpoint-concurrent");
    let first = Arc::new(FileSyncCheckpointStore::new(&path).unwrap());
    let second = Arc::new(FileSyncCheckpointStore::new(&path).unwrap());
    let handles = (0..40)
        .map(|index| {
            let store = if index % 2 == 0 {
                Arc::clone(&first)
            } else {
                Arc::clone(&second)
            };
            thread::spawn(move || {
                store
                    .set_checkpoint(&format!("peer-{index}"), index + 1, TEST_HASH_A)
                    .unwrap();
            })
        })
        .collect::<Vec<_>>();
    for handle in handles {
        handle.join().unwrap();
    }
    let reloaded = FileSyncCheckpointStore::new(&path).unwrap();
    for index in 0..40 {
        assert_eq!(
            reloaded.get_last_sequence(&format!("peer-{index}")),
            Ok(index + 1)
        );
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn checkpoint_ignores_orphaned_atomic_write_stages() {
    let path = unique_path("checkpoint-crash-stages");
    let store = FileSyncCheckpointStore::new(&path).unwrap();
    store.set_checkpoint("peer-a", 1, TEST_HASH_A).unwrap();
    let temporary = path.with_extension("checkpoint.tmp");
    for bytes in [
        Vec::new(),
        b"# appcore-sync-checkpoint-v1\npeer-a=".to_vec(),
        format!("{SYNC_CHECKPOINT_FORMAT_V1}\npeer-a=2,{TEST_HASH_B}\n").into_bytes(),
    ] {
        std::fs::write(&temporary, bytes).unwrap();
        let recovered = FileSyncCheckpointStore::new(&path).unwrap();
        assert_eq!(
            recovered.get_checkpoint("peer-a"),
            Ok(Some((1, TEST_HASH_A.into())))
        );
        std::fs::remove_file(&temporary).unwrap();
    }
    std::fs::write(
        &path,
        format!("{SYNC_CHECKPOINT_FORMAT_V1}\npeer-a=2,{TEST_HASH_B}\n"),
    )
    .unwrap();
    let committed = FileSyncCheckpointStore::new(&path).unwrap();
    assert_eq!(
        committed.get_checkpoint("peer-a"),
        Ok(Some((2, TEST_HASH_B.into())))
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn file_checkpoint_store_rejects_invalid_peer_id() {
    let path = unique_path("checkpoint-invalid-peer");
    let store = FileSyncCheckpointStore::new(path.clone());
    assert!(store.is_ok());
    let store = match store {
        Ok(store) => store,
        Err(_) => return,
    };
    let result = store.set_last_sequence("../bad", 1);
    assert_eq!(result, Err(super::SyncError::InvalidPeerId));
    let _ = std::fs::remove_file(path);
}

#[test]
fn file_checkpoint_store_rejects_corrupted_persisted_sequence() {
    let path = unique_path("checkpoint-corrupt-sequence");
    assert!(std::fs::write(&path, format!("{SYNC_CHECKPOINT_FORMAT_V1}\npeer-a=bad\n")).is_ok());
    assert_eq!(
        FileSyncCheckpointStore::new(path.clone()).map(|_| ()),
        Err(super::SyncError::ReplicationFailed(
            "invalid checkpoint sequence".to_string()
        ))
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn receiver_skips_old_sequence_after_restart_simulation() {
    let path = unique_path("checkpoint-restart");
    let checkpoint = FileSyncCheckpointStore::new(path.clone());
    assert!(checkpoint.is_ok());
    let checkpoint = match checkpoint {
        Ok(store) => Arc::new(store),
        Err(_) => return,
    };
    checkpoint.set_last_sequence("leader-a", 19).unwrap();

    let first_log: Arc<Mutex<Box<dyn ReplicationLog + Send>>> =
        Arc::new(Mutex::new(Box::new(InMemoryReplicationLog::new())));
    let first_state = SyncReceiverState::new(Arc::clone(&first_log), checkpoint.clone());
    let first_message = SyncMessage::new(
        "batch-restart-1".to_string(),
        NodeId::new("leader-a".to_string()).unwrap(),
        20,
        20,
        0,
        None,
        vec![b"a".to_vec()],
    );
    assert!(first_state.apply_sync_message(&first_message).is_ok());

    let restarted_log: Arc<Mutex<Box<dyn ReplicationLog + Send>>> =
        Arc::new(Mutex::new(Box::new(InMemoryReplicationLog::new())));
    let restarted_state = SyncReceiverState::new(Arc::clone(&restarted_log), checkpoint);
    let duplicate = SyncMessage::new(
        "batch-restart-2".to_string(),
        NodeId::new("leader-a".to_string()).unwrap(),
        20,
        20,
        0,
        None,
        vec![b"a".to_vec()],
    );
    let ack = restarted_state.apply_sync_message(&duplicate);
    assert!(ack.is_ok());
    let ack = match ack {
        Ok(value) => value,
        Err(_) => return,
    };
    assert_eq!(ack.received, 0);
    assert_eq!(ack.skipped, 1);
    let len = restarted_log.lock().len();
    assert_eq!(len, Ok(0));
    let _ = std::fs::remove_file(path);
}

#[test]
fn receiver_rejects_zero_sequence_without_mutating_log() {
    let checkpoint = Arc::new(InMemorySyncCheckpointStore::new());
    let log: Arc<Mutex<Box<dyn ReplicationLog + Send>>> =
        Arc::new(Mutex::new(Box::new(InMemoryReplicationLog::new())));
    let state = SyncReceiverState::new(Arc::clone(&log), checkpoint);
    let invalid = SyncMessage::new_simple(
        NodeId::new("leader-a".to_string()).unwrap(),
        0,
        vec![b"a".to_vec()],
    );

    assert_eq!(
        state.apply_sync_message(&invalid),
        Err(super::SyncError::InvalidSequence(0))
    );
    assert_eq!(log.lock().len(), Ok(0));
}

#[test]
fn receiver_skips_duplicate_batch_without_mutating_log() {
    let checkpoint = Arc::new(InMemorySyncCheckpointStore::new());
    checkpoint.set_last_sequence("leader-a", 6).unwrap();
    let log: Arc<Mutex<Box<dyn ReplicationLog + Send>>> =
        Arc::new(Mutex::new(Box::new(InMemoryReplicationLog::new())));
    let state = SyncReceiverState::new(Arc::clone(&log), checkpoint);
    let batch = SyncMessage::new_simple(
        NodeId::new("leader-a".to_string()).unwrap(),
        7,
        vec![b"a".to_vec(), b"b".to_vec()],
    );

    assert_eq!(
        state.apply_sync_message(&batch).map(|ack| ack.received),
        Ok(2)
    );
    assert_eq!(
        state.apply_sync_message(&batch),
        Err(super::SyncError::InvalidSyncMessage("duplicate batch_id"))
    );

    let batch2 = SyncMessage::new(
        "batch-7-diff".to_string(),
        NodeId::new("leader-a".to_string()).unwrap(),
        7,
        8,
        0,
        None,
        vec![b"a".to_vec(), b"b".to_vec()],
    );
    let ack2 = state.apply_sync_message(&batch2);
    assert!(ack2.is_ok());
    let ack2 = ack2.unwrap();
    assert_eq!(ack2.received, 0);
    assert_eq!(ack2.skipped, 2);

    assert_eq!(log.lock().len(), Ok(2));
}

#[test]
fn file_replication_log_appends_and_reloads() {
    let root = std::env::temp_dir().join(format!(
        "appcore-sync-log-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ));
    assert!(std::fs::create_dir_all(&root).is_ok());
    let mut log = match FileReplicationLog::new(&root, "sync-replication.log") {
        Ok(log) => log,
        Err(_) => return,
    };
    assert!(log.append_with_sequence(b"e1".to_vec(), 1).is_ok());
    assert!(log.append_with_sequence(b"e2".to_vec(), 2).is_ok());
    assert_eq!(log.last_index(), Ok(2));

    let reloaded = match FileReplicationLog::new(&root, "sync-replication.log") {
        Ok(log) => log,
        Err(_) => return,
    };
    assert_eq!(reloaded.len(), Ok(2));
    let events = reloaded.events_since(1);
    assert!(events.is_ok());
    let events = events.unwrap_or_default();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0], b"e2".to_vec());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn concurrent_replication_log_instances_preserve_every_record() {
    let root = unique_path("replication-concurrent");
    std::fs::create_dir_all(&root).unwrap();
    let first = FileReplicationLog::new(&root, "replication.log").unwrap();
    let second = FileReplicationLog::new(&root, "replication.log").unwrap();
    let first_handle = thread::spawn(move || {
        let mut log = first;
        for sequence in (1..=40).step_by(2) {
            log.append_with_sequence(format!("event-{sequence}").into_bytes(), sequence)
                .unwrap();
        }
    });
    let second_handle = thread::spawn(move || {
        let mut log = second;
        for sequence in (2..=40).step_by(2) {
            log.append_with_sequence(format!("event-{sequence}").into_bytes(), sequence)
                .unwrap();
        }
    });
    first_handle.join().unwrap();
    second_handle.join().unwrap();
    let reloaded = FileReplicationLog::new(&root, "replication.log").unwrap();
    assert_eq!(reloaded.len(), Ok(40));
    for sequence in 1..=40 {
        assert_eq!(
            reloaded.event_at_sequence(sequence).unwrap(),
            Some(format!("event-{sequence}").into_bytes())
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn replication_log_replays_same_sequence_and_rejects_divergence() {
    let mut log = InMemoryReplicationLog::new();
    assert_eq!(log.append_with_sequence(b"e1".to_vec(), 1), Ok(1));
    assert_eq!(log.append_with_sequence(b"e1".to_vec(), 1), Ok(1));
    assert_eq!(log.len(), Ok(1));
    assert_eq!(
        log.append_with_sequence(b"different".to_vec(), 1),
        Err(super::SyncError::SequenceConflict(1))
    );
}

#[test]
fn in_memory_replication_snapshot_roundtrips_and_rejects_tampering() {
    let mut source = InMemoryReplicationLog::new();
    source.append_with_sequence(b"e1".to_vec(), 1).unwrap();
    source.append_with_sequence(b"e2".to_vec(), 2).unwrap();
    let snapshot = source.create_snapshot().unwrap();

    let mut restored = InMemoryReplicationLog::new();
    restored.restore_snapshot(&snapshot).unwrap();
    assert_eq!(
        restored.events_since(0).unwrap(),
        source.events_since(0).unwrap()
    );

    let mut tampered = snapshot;
    tampered.records[0].payload = b"tampered".to_vec();
    assert_eq!(
        restored.restore_snapshot(&tampered),
        Err(super::SyncError::InvalidSnapshot("checksum mismatch"))
    );
}

#[test]
fn file_replication_snapshot_restore_is_durable() {
    let root = unique_path("snapshot-root");
    std::fs::create_dir_all(&root).unwrap();
    let mut source = InMemoryReplicationLog::new();
    source.append_with_sequence(b"e1".to_vec(), 1).unwrap();
    source.append_with_sequence(b"e2".to_vec(), 2).unwrap();
    let snapshot = source.create_snapshot().unwrap();

    let mut file_log = FileReplicationLog::new(&root, "replication.log").unwrap();
    file_log.append_with_sequence(b"old".to_vec(), 1).unwrap();
    file_log.restore_snapshot(&snapshot).unwrap();
    assert_eq!(
        file_log.events_since(0).unwrap(),
        source.events_since(0).unwrap()
    );

    let reloaded = FileReplicationLog::new(&root, "replication.log").unwrap();
    assert_eq!(
        reloaded.events_since(0).unwrap(),
        source.events_since(0).unwrap()
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn snapshot_restore_ignores_orphaned_atomic_write_stages() {
    let root = unique_path("snapshot-crash-stages");
    let source_root = unique_path("snapshot-crash-source");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&source_root).unwrap();
    let mut stable = FileReplicationLog::new(&root, "replication.log").unwrap();
    stable.append_with_sequence(b"old".to_vec(), 1).unwrap();
    let mut source = FileReplicationLog::new(&source_root, "replication.log").unwrap();
    source.append_with_sequence(b"new-a".to_vec(), 1).unwrap();
    source.append_with_sequence(b"new-b".to_vec(), 2).unwrap();
    let committed_bytes = std::fs::read(source_root.join("replication.log")).unwrap();
    let temporary = root.join(".replication.log.crash.tmp");
    for bytes in [
        Vec::new(),
        committed_bytes[..committed_bytes.len() / 2].to_vec(),
        committed_bytes.clone(),
    ] {
        std::fs::write(&temporary, bytes).unwrap();
        let recovered = FileReplicationLog::new(&root, "replication.log").unwrap();
        assert_eq!(recovered.events_since(0).unwrap(), vec![b"old".to_vec()]);
        std::fs::remove_file(&temporary).unwrap();
    }
    std::fs::write(root.join("replication.log"), committed_bytes).unwrap();
    let committed = FileReplicationLog::new(&root, "replication.log").unwrap();
    assert_eq!(
        committed.events_since(0).unwrap(),
        vec![b"new-a".to_vec(), b"new-b".to_vec()]
    );
    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_dir_all(source_root).unwrap();
}

#[test]
fn file_sync_outbox_survives_restart_and_acknowledgement() {
    let path = unique_path("outbox");
    let message = SyncMessage::new_simple(
        NodeId::new("leader-a".to_string()).unwrap(),
        1,
        vec![b"e1".to_vec()],
    );
    let outbox = FileSyncOutbox::new(&path).unwrap();
    assert_eq!(outbox.try_enqueue(message.clone(), 8), Ok(true));
    drop(outbox);

    let reloaded = FileSyncOutbox::new(&path).unwrap();
    assert_eq!(reloaded.front(), Ok(Some(message.clone())));
    assert_eq!(reloaded.acknowledge_front(&message.batch_id), Ok(()));
    drop(reloaded);

    let empty = FileSyncOutbox::new(&path).unwrap();
    assert_eq!(empty.len(), Ok(0));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn file_outbox_paging_retry_and_partial_receipts_survive_restart() {
    let path = unique_path("outbox-paging-retry");
    let messages = (1..=3)
        .map(|sequence| {
            SyncMessage::new_simple(
                NodeId::new("file-page-node".to_string()).unwrap(),
                sequence,
                vec![vec![sequence as u8; sequence as usize * 32]],
            )
        })
        .collect::<Vec<_>>();
    let outbox = FileSyncOutbox::new(&path).unwrap();
    for message in &messages {
        assert_eq!(outbox.try_enqueue(message.clone(), 8), Ok(true));
    }
    let first_bytes = serde_json::to_vec(&messages[0]).unwrap().len();
    let second_bytes = serde_json::to_vec(&messages[1]).unwrap().len();
    assert_eq!(
        outbox.peek(3, first_bytes + second_bytes).unwrap(),
        messages[..2]
    );
    assert!(outbox.mark_attempt(&messages[1].batch_id, 500).is_err());
    assert_eq!(outbox.mark_attempt(&messages[0].batch_id, 500), Ok(1));
    assert_eq!(outbox.mark_attempt(&messages[0].batch_id, 750), Ok(2));
    drop(outbox);

    let reloaded = FileSyncOutbox::new(&path).unwrap();
    assert!(reloaded
        .next_ready(749, 3, 1_024 * 1_024)
        .unwrap()
        .is_empty());
    assert_eq!(
        reloaded.next_ready(750, 3, 1_024 * 1_024).unwrap(),
        messages
    );
    let stats = reloaded.stats().unwrap();
    assert_eq!(stats.pending_messages, 3);
    assert_eq!(stats.attempted_messages, Some(1));
    assert_eq!(stats.total_attempts, Some(2));
    assert_eq!(stats.next_ready_at_ms, Some(750));

    let wrong = SyncOutboxReceipt::new(vec![messages[1].batch_id.clone()]).unwrap();
    let before_wrong = std::fs::read(&path).unwrap();
    assert!(reloaded.acknowledge_receipt(&wrong).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before_wrong);
    let partial = SyncOutboxReceipt::new(
        messages[..2]
            .iter()
            .map(|message| message.batch_id.clone())
            .collect(),
    )
    .unwrap();
    assert_eq!(reloaded.acknowledge_receipt(&partial), Ok(2));
    drop(reloaded);

    let remaining = FileSyncOutbox::new(&path).unwrap();
    assert_eq!(remaining.front(), Ok(Some(messages[2].clone())));
    assert_eq!(remaining.stats().unwrap().pending_messages, 1);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn file_outbox_attempt_tampering_fails_closed() {
    let path = unique_path("outbox-attempt-tamper");
    let message = SyncMessage::new_simple(
        NodeId::new("attempt-node".to_string()).unwrap(),
        1,
        vec![b"event".to_vec()],
    );
    let outbox = FileSyncOutbox::new(&path).unwrap();
    outbox.try_enqueue(message.clone(), 8).unwrap();
    outbox.mark_attempt(&message.batch_id, 500).unwrap();
    drop(outbox);

    let mut bytes = std::fs::read(&path).unwrap();
    let marker = message.batch_id.as_bytes();
    let offset = bytes
        .windows(marker.len())
        .rposition(|window| window == marker)
        .unwrap();
    bytes[offset] ^= 1;
    std::fs::write(&path, &bytes).unwrap();
    assert!(matches!(
        FileSyncOutbox::new(&path),
        Err(SyncError::CorruptOutbox { .. })
    ));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn outbox_enqueue_and_ack_append_without_rewriting_the_journal() {
    let path = unique_path("outbox-incremental");
    let first = SyncMessage::new_simple(
        NodeId::new("leader-a".to_string()).unwrap(),
        1,
        vec![b"first".to_vec()],
    );
    let second = SyncMessage::new_simple(
        NodeId::new("leader-a".to_string()).unwrap(),
        2,
        vec![b"second".to_vec()],
    );
    let outbox = FileSyncOutbox::new(&path).unwrap();
    outbox.try_enqueue(first.clone(), 8).unwrap();
    let after_enqueue = std::fs::read(&path).unwrap();
    outbox.acknowledge_front(&first.batch_id).unwrap();
    let after_ack = std::fs::read(&path).unwrap();
    outbox.try_enqueue(second.clone(), 8).unwrap();
    let after_second = std::fs::read(&path).unwrap();
    let header_bytes = SYNC_OUTBOX_FORMAT_V2.len() + 1 + 16 + 32;
    assert!(after_ack.len() > after_enqueue.len());
    assert!(after_second.len() > after_ack.len());
    assert_eq!(
        &after_enqueue[..header_bytes],
        &after_second[..header_bytes]
    );
    assert_eq!(outbox.front(), Ok(Some(second)));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn outbox_recovers_only_an_incomplete_final_frame() {
    let path = unique_path("outbox-partial-tail");
    let completed_path = unique_path("outbox-completed-ack");
    let message = SyncMessage::new_simple(
        NodeId::new("leader-a".to_string()).unwrap(),
        1,
        vec![b"event".to_vec()],
    );
    let outbox = FileSyncOutbox::new(&path).unwrap();
    outbox.try_enqueue(message.clone(), 8).unwrap();
    let committed_len = std::fs::metadata(&path).unwrap().len() as usize;

    let completed = FileSyncOutbox::new(&completed_path).unwrap();
    completed.try_enqueue(message.clone(), 8).unwrap();
    completed.acknowledge_front(&message.batch_id).unwrap();
    let completed_bytes = std::fs::read(&completed_path).unwrap();
    let ack_frame = &completed_bytes[committed_len..];
    assert!(!ack_frame.is_empty());

    use std::io::Write as _;
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    file.write_all(&ack_frame[..ack_frame.len() / 2]).unwrap();
    file.sync_all().unwrap();
    drop(file);
    assert_eq!(
        FileSyncOutbox::new(&path).unwrap().front(),
        Ok(Some(message.clone()))
    );
    assert_eq!(
        std::fs::metadata(&path).unwrap().len() as usize,
        committed_len
    );

    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    file.write_all(ack_frame).unwrap();
    file.sync_all().unwrap();
    drop(file);
    assert_eq!(FileSyncOutbox::new(&path).unwrap().len(), Ok(0));
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(completed_path).unwrap();
}

#[test]
fn outbox_rejects_complete_record_corruption_without_repairing_it() {
    let path = unique_path("outbox-corrupt-frame");
    let message = SyncMessage::new_simple(
        NodeId::new("leader-a".to_string()).unwrap(),
        1,
        vec![b"event".to_vec()],
    );
    FileSyncOutbox::new(&path)
        .unwrap()
        .try_enqueue(message, 8)
        .unwrap();
    let mut bytes = std::fs::read(&path).unwrap();
    let header_bytes = SYNC_OUTBOX_FORMAT_V2.len() + 1 + 16 + 32;
    let data_offset = header_bytes + 4 + 8 + 1 + 4;
    bytes[data_offset + 1] ^= 0x01;
    std::fs::write(&path, &bytes).unwrap();
    assert!(matches!(
        FileSyncOutbox::new(&path),
        Err(super::SyncError::CorruptOutbox { .. })
    ));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn outbox_rejects_duplicated_complete_frame() {
    let path = unique_path("outbox-duplicate-frame");
    let message = SyncMessage::new_simple(
        NodeId::new("leader-a".to_string()).unwrap(),
        1,
        vec![b"event".to_vec()],
    );
    FileSyncOutbox::new(&path)
        .unwrap()
        .try_enqueue(message, 8)
        .unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let header_bytes = SYNC_OUTBOX_FORMAT_V2.len() + 1 + 16 + 32;
    let first_frame = &bytes[header_bytes..];
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    file.write_all(first_frame).unwrap();
    file.sync_all().unwrap();
    drop(file);

    assert!(matches!(
        FileSyncOutbox::new(&path),
        Err(super::SyncError::CorruptOutbox { .. })
    ));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn outbox_debug_does_not_expose_pending_payloads_or_batch_ids() {
    let path = unique_path("outbox-debug-redaction");
    let mut message = SyncMessage::new_simple(
        NodeId::new("leader-a".to_string()).unwrap(),
        1,
        vec![b"private-payload-marker".to_vec()],
    );
    message.batch_id = "private-batch-marker".to_string();
    let outbox = FileSyncOutbox::new(&path).unwrap();
    outbox.try_enqueue(message, 8).unwrap();

    let debug = format!("{outbox:?}");
    assert!(!debug.contains("private-payload-marker"));
    assert!(!debug.contains("private-batch-marker"));
    assert!(debug.contains("pending_messages"));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn outbox_compaction_is_atomic_and_stale_instances_reload_generation() {
    let path = unique_path("outbox-compaction");
    let first = FileSyncOutbox::new(&path).unwrap();
    let stale = FileSyncOutbox::new(&path).unwrap();
    let large = SyncMessage::new_simple(
        NodeId::new("leader-a".to_string()).unwrap(),
        1,
        vec![vec![b'x'; 2 * 1024 * 1024]],
    );
    let small = SyncMessage::new_simple(
        NodeId::new("leader-a".to_string()).unwrap(),
        2,
        vec![b"small".to_vec()],
    );
    first.try_enqueue(large.clone(), 8).unwrap();
    let before_header = std::fs::read(&path).unwrap();
    first.acknowledge_front(&large.batch_id).unwrap();
    first.try_enqueue(small.clone(), 8).unwrap();
    let compacted = std::fs::read(&path).unwrap();
    let header_bytes = SYNC_OUTBOX_FORMAT_V2.len() + 1 + 16 + 32;
    assert_ne!(&before_header[..header_bytes], &compacted[..header_bytes]);
    assert!(compacted.len() < before_header.len() / 4);
    assert_eq!(stale.messages(), Ok(vec![small]));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn concurrent_outbox_instances_do_not_lose_messages() {
    let path = unique_path("outbox-concurrent");
    let first = Arc::new(FileSyncOutbox::new(&path).unwrap());
    let second = Arc::new(FileSyncOutbox::new(&path).unwrap());
    let handles = (1..=40)
        .map(|sequence| {
            let outbox = if sequence % 2 == 0 {
                Arc::clone(&first)
            } else {
                Arc::clone(&second)
            };
            thread::spawn(move || {
                let message = SyncMessage::new_simple(
                    NodeId::new("leader-a".to_string()).unwrap(),
                    sequence,
                    vec![format!("event-{sequence}").into_bytes()],
                );
                assert_eq!(outbox.try_enqueue(message, 100), Ok(true));
            })
        })
        .collect::<Vec<_>>();
    for handle in handles {
        handle.join().unwrap();
    }
    let reloaded = FileSyncOutbox::new(&path).unwrap();
    assert_eq!(reloaded.len(), Ok(40));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn outbox_ignores_orphaned_atomic_write_stages() {
    let path = unique_path("outbox-crash-stages");
    let committed_path = unique_path("outbox-crash-committed");
    let old = SyncMessage::new_simple(
        NodeId::new("leader-a".to_string()).unwrap(),
        1,
        vec![b"old".to_vec()],
    );
    let new = SyncMessage::new_simple(
        NodeId::new("leader-a".to_string()).unwrap(),
        2,
        vec![b"new".to_vec()],
    );
    FileSyncOutbox::new(&path)
        .unwrap()
        .try_enqueue(old.clone(), 8)
        .unwrap();
    FileSyncOutbox::new(&committed_path)
        .unwrap()
        .try_enqueue(new.clone(), 8)
        .unwrap();
    let committed_bytes = std::fs::read(&committed_path).unwrap();
    let temporary = path.with_extension("outbox.tmp");
    for bytes in [
        Vec::new(),
        committed_bytes[..committed_bytes.len() / 2].to_vec(),
        committed_bytes.clone(),
    ] {
        std::fs::write(&temporary, bytes).unwrap();
        assert_eq!(
            FileSyncOutbox::new(&path).unwrap().front(),
            Ok(Some(old.clone()))
        );
        std::fs::remove_file(&temporary).unwrap();
    }
    std::fs::write(&path, committed_bytes).unwrap();
    assert_eq!(FileSyncOutbox::new(&path).unwrap().front(), Ok(Some(new)));
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(committed_path).unwrap();
}

#[test]
fn persistent_sync_formats_reject_unversioned_files() {
    let checkpoint = unique_path("checkpoint-unversioned");
    std::fs::write(&checkpoint, "peer-a=7,\n").unwrap();
    assert_eq!(
        FileSyncCheckpointStore::new(&checkpoint).map(|_| ()),
        Err(super::SyncError::ReplicationFailed(
            "NO MORE SUPPORTED PLEASE UPDATE".to_string()
        ))
    );

    let outbox_path = unique_path("outbox-unversioned");
    let message = SyncMessage::new_simple(
        NodeId::new("leader-a".to_string()).unwrap(),
        1,
        vec![b"e1".to_vec()],
    );
    let outbox = FileSyncOutbox::new(&outbox_path).unwrap();
    outbox.try_enqueue(message, 8).unwrap();
    assert!(std::fs::read(&outbox_path)
        .unwrap()
        .starts_with(SYNC_OUTBOX_FORMAT_V2.as_bytes()));
    std::fs::write(&outbox_path, b"# appcore-sync-outbox-v1\n").unwrap();
    assert_eq!(
        FileSyncOutbox::new(&outbox_path).map(|_| ()),
        Err(super::SyncError::ReplicationFailed(
            "NO MORE SUPPORTED PLEASE UPDATE".to_string()
        ))
    );

    let root = unique_path("replication-unversioned");
    std::fs::create_dir_all(&root).unwrap();
    let log_path = root.join("replication.log");
    std::fs::write(&log_path, "1\t6531\n2\t6532\n").unwrap();
    assert_eq!(
        FileReplicationLog::new(&root, "replication.log"),
        Err(super::SyncError::ReplicationFailed(
            "NO MORE SUPPORTED PLEASE UPDATE".to_string()
        ))
    );

    std::fs::remove_file(checkpoint).unwrap();
    std::fs::remove_file(outbox_path).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn replication_log_rejects_hash_chain_tampering() {
    let root = unique_path("replication-hash-tamper");
    std::fs::create_dir_all(&root).unwrap();
    let mut log = FileReplicationLog::new(&root, "replication.log").unwrap();
    log.append_with_sequence(b"first".to_vec(), 1).unwrap();
    let path = root.join("replication.log");
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, text.replacen("6669727374", "74616d706572", 1)).unwrap();

    assert!(matches!(
        FileReplicationLog::new(&root, "replication.log"),
        Err(super::SyncError::CorruptReplicationLog {
            reason: "hash chain mismatch",
            ..
        })
    ));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn replication_log_discards_partial_append_tail_after_crash() {
    let root = unique_path("replication-partial-tail");
    std::fs::create_dir_all(&root).unwrap();
    let mut log = FileReplicationLog::new(&root, "replication.log").unwrap();
    log.append_with_sequence(b"complete".to_vec(), 1).unwrap();
    let path = root.join("replication.log");
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    file.write_all(b"2\t706172").unwrap();
    drop(file);

    let recovered = FileReplicationLog::new(&root, "replication.log").unwrap();
    assert_eq!(
        recovered.events_since(0).unwrap(),
        vec![b"complete".to_vec()]
    );
    assert!(!std::fs::read_to_string(path).unwrap().contains("706172"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn persistent_sync_formats_reject_future_versions() {
    let path = unique_path("checkpoint-future");
    std::fs::write(&path, "# appcore-sync-checkpoint-v2\n").unwrap();
    assert!(FileSyncCheckpointStore::new(&path).is_err());
    std::fs::remove_file(path).unwrap();

    let outbox_path = unique_path("outbox-future");
    std::fs::write(&outbox_path, b"appcore-sync-outbox-v3\0").unwrap();
    assert_eq!(
        FileSyncOutbox::new(&outbox_path).map(|_| ()),
        Err(super::SyncError::ReplicationFailed(
            "NO MORE SUPPORTED PLEASE UPDATE".to_string()
        ))
    );
    std::fs::remove_file(outbox_path).unwrap();
}

#[test]
fn receiver_recovers_when_checkpoint_fails_after_append() {
    let checkpoint = Arc::new(FailOnceCheckpointStore::new());
    let log: Arc<Mutex<Box<dyn ReplicationLog + Send>>> =
        Arc::new(Mutex::new(Box::new(InMemoryReplicationLog::new())));
    let state = SyncReceiverState::new(Arc::clone(&log), checkpoint.clone());
    let batch = SyncMessage::new(
        "checkpoint-recovery".to_string(),
        NodeId::new("leader-a".to_string()).unwrap(),
        1,
        2,
        0,
        None,
        vec![b"e1".to_vec(), b"e2".to_vec()],
    );

    assert_eq!(
        state.apply_sync_message(&batch),
        Err(super::SyncError::ReplicationFailed(
            "injected checkpoint failure".to_string()
        ))
    );
    assert_eq!(log.lock().len(), Ok(2));

    let ack = state.apply_sync_message(&batch).unwrap();
    assert_eq!(ack.received, 0);
    assert_eq!(ack.skipped, 2);
    assert_eq!(ack.last_sequence, 2);
    assert_eq!(log.lock().len(), Ok(2));
    assert_eq!(checkpoint.get_last_sequence("leader-a"), Ok(2));
}

#[test]
fn file_replication_log_rejects_invalid_path() {
    let root = std::env::temp_dir().join(format!(
        "appcore-sync-log-invalid-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ));
    assert!(std::fs::create_dir_all(&root).is_ok());
    let result = FileReplicationLog::new(&root, "../escape.log");
    assert!(result.is_err());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn file_replication_log_rejects_corrupted_line() {
    let root = std::env::temp_dir().join(format!(
        "appcore-sync-corrupt-log-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ));
    assert!(std::fs::create_dir_all(&root).is_ok());
    assert!(std::fs::write(
        root.join("sync-replication.log"),
        format!("{REPLICATION_LOG_FORMAT_V1}\nbad-line\n")
    )
    .is_ok());
    assert_eq!(
        FileReplicationLog::new(&root, "sync-replication.log"),
        Err(super::SyncError::CorruptReplicationLog {
            line: 1,
            reason: "missing separator"
        })
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn file_replication_log_rejects_corrupted_sequence() {
    let root = std::env::temp_dir().join(format!(
        "appcore-sync-corrupt-sequence-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ));
    assert!(std::fs::create_dir_all(&root).is_ok());
    assert!(std::fs::write(
        root.join("sync-replication.log"),
        format!("{REPLICATION_LOG_FORMAT_V1}\nbad\t61\t\tbad\n")
    )
    .is_ok());
    assert_eq!(
        FileReplicationLog::new(&root, "sync-replication.log"),
        Err(super::SyncError::CorruptReplicationLog {
            line: 1,
            reason: "invalid sequence"
        })
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn file_replication_log_rejects_corrupted_event_hex() {
    let root = std::env::temp_dir().join(format!(
        "appcore-sync-corrupt-hex-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ));
    assert!(std::fs::create_dir_all(&root).is_ok());
    assert!(std::fs::write(
        root.join("sync-replication.log"),
        format!("{REPLICATION_LOG_FORMAT_V1}\n1\tzz\t\tbad\n")
    )
    .is_ok());
    assert_eq!(
        FileReplicationLog::new(&root, "sync-replication.log"),
        Err(super::SyncError::CorruptReplicationLog {
            line: 1,
            reason: "invalid event hex"
        })
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn receiver_rejects_inconsistent_sequence_range() {
    let checkpoint = Arc::new(InMemorySyncCheckpointStore::new());
    let log: Arc<Mutex<Box<dyn ReplicationLog + Send>>> =
        Arc::new(Mutex::new(Box::new(InMemoryReplicationLog::new())));
    let state = SyncReceiverState::new(Arc::clone(&log), checkpoint);
    let batch = SyncMessage::new(
        "batch-inconsistent-range".to_string(),
        NodeId::new("leader-a".to_string()).unwrap(),
        10,
        12, // range requires 3 events
        0,
        None,
        vec![b"a".to_vec(), b"b".to_vec()], // only 2 events
    );
    assert_eq!(
        state.apply_sync_message(&batch),
        Err(super::SyncError::InvalidSyncMessage(
            "inconsistent sequence range"
        ))
    );
}

#[test]
fn receiver_rejects_oversized_event_before_mutating_the_log() {
    let checkpoint = Arc::new(InMemorySyncCheckpointStore::new());
    let log: Arc<Mutex<Box<dyn ReplicationLog + Send>>> =
        Arc::new(Mutex::new(Box::new(InMemoryReplicationLog::new())));
    let state = SyncReceiverState::new(Arc::clone(&log), checkpoint.clone());
    let batch = SyncMessage::new_simple(
        NodeId::new("leader-a").unwrap(),
        1,
        vec![b"valid".to_vec(), vec![0; 1024 * 1024 + 1]],
    );

    assert!(state.apply_sync_message(&batch).is_err());
    assert_eq!(log.lock().len(), Ok(0));
    assert_eq!(checkpoint.get_last_sequence("leader-a").unwrap(), 0);
}

#[test]
fn receiver_rejects_sequence_overflow_without_panicking() {
    let checkpoint = Arc::new(InMemorySyncCheckpointStore::new());
    let log: Arc<Mutex<Box<dyn ReplicationLog + Send>>> =
        Arc::new(Mutex::new(Box::new(InMemoryReplicationLog::new())));
    let state = SyncReceiverState::new(log, checkpoint);
    let batch = SyncMessage::new_simple(
        NodeId::new("leader-a").unwrap(),
        u64::MAX,
        vec![b"first".to_vec(), b"second".to_vec()],
    );

    assert_eq!(
        state.apply_sync_message(&batch),
        Err(super::SyncError::InvalidSyncMessage(
            "sequence range overflow"
        ))
    );
}

#[test]
fn checkpoint_rejects_oversized_ids_and_malformed_hashes() {
    let checkpoint = InMemorySyncCheckpointStore::new();
    assert!(checkpoint
        .set_checkpoint(&"a".repeat(257), 1, TEST_HASH_A)
        .is_err());
    assert!(checkpoint
        .set_checkpoint("peer-a", 1, "not-a-hash")
        .is_err());
}

#[test]
fn receiver_processed_batches_limits_and_duplication() {
    let checkpoint = Arc::new(InMemorySyncCheckpointStore::new());
    checkpoint.set_last_sequence("leader-a", 9).unwrap();
    let log: Arc<Mutex<Box<dyn ReplicationLog + Send>>> =
        Arc::new(Mutex::new(Box::new(InMemoryReplicationLog::new())));
    let state = SyncReceiverState::new(Arc::clone(&log), checkpoint);

    let batch1 = SyncMessage::new_simple(
        NodeId::new("leader-a".to_string()).unwrap(),
        10,
        vec![b"a".to_vec()],
    );
    assert!(state.apply_sync_message(&batch1).is_ok());

    assert_eq!(
        state.apply_sync_message(&batch1),
        Err(super::SyncError::InvalidSyncMessage("duplicate batch_id"))
    );
}

#[test]
fn receiver_hardening_explicit_rejections() {
    let checkpoint = Arc::new(InMemorySyncCheckpointStore::new());
    let log: Arc<Mutex<Box<dyn ReplicationLog + Send>>> =
        Arc::new(Mutex::new(Box::new(InMemoryReplicationLog::new())));
    let state = SyncReceiverState::new(Arc::clone(&log), checkpoint.clone());

    // 1. Initial contiguous batch 1..3 accepted
    let batch1 = SyncMessage::new(
        "batch-1".to_string(),
        NodeId::new("leader-a".to_string()).unwrap(),
        1,
        3,
        0,
        None,
        vec![b"e1".to_vec(), b"e2".to_vec(), b"e3".to_vec()],
    );
    let ack = state.apply_sync_message(&batch1).unwrap();
    assert!(ack.accepted);
    assert_eq!(ack.received, 3);
    assert_eq!(ack.skipped, 0);
    assert_eq!(ack.last_sequence, 3);

    // 2. Replay of already applied batch (same batch_id) -> rejected as duplicate
    assert_eq!(
        state.apply_sync_message(&batch1),
        Err(super::SyncError::InvalidSyncMessage("duplicate batch_id"))
    );

    // 2b. Replay of already applied sequence range but with a different batch_id -> skipped
    let batch1_replay = SyncMessage::new(
        "batch-1-replay".to_string(),
        NodeId::new("leader-a".to_string()).unwrap(),
        1,
        3,
        0,
        None,
        vec![b"e1".to_vec(), b"e2".to_vec(), b"e3".to_vec()],
    );
    let replay_ack = state.apply_sync_message(&batch1_replay).unwrap();
    assert!(replay_ack.accepted);
    assert_eq!(replay_ack.received, 0);
    assert_eq!(replay_ack.skipped, 3);
    assert_eq!(replay_ack.last_sequence, 3);

    // 3. Divergent partial overlap is rejected without advancing the checkpoint.
    let divergent_overlap = SyncMessage::new(
        "batch-overlap-divergent".to_string(),
        NodeId::new("leader-a".to_string()).unwrap(),
        3,
        4,
        0,
        None,
        vec![b"different".to_vec(), b"e4".to_vec()],
    );
    assert_eq!(
        state.apply_sync_message(&divergent_overlap),
        Err(super::SyncError::SequenceConflict(3))
    );

    // 4. Gap (starts at 5, ends at 5, last_seq is 3) - rejected (since 4 is missing)
    let gap_batch = SyncMessage::new(
        "batch-gap".to_string(),
        NodeId::new("leader-a".to_string()).unwrap(),
        5,
        5,
        0,
        None,
        vec![b"e5".to_vec()],
    );
    assert!(state.apply_sync_message(&gap_batch).is_err());

    // 5. Previous batch hash mismatch (starts at 4, but hash chain mismatched)
    let mismatch_batch = SyncMessage::new(
        "batch-4-mismatch".to_string(),
        NodeId::new("leader-a".to_string()).unwrap(),
        4,
        4,
        0,
        Some("divergent_hash".to_string()),
        vec![b"e4".to_vec()],
    );
    assert_eq!(
        state.apply_sync_message(&mismatch_batch),
        Err(super::SyncError::InvalidSyncMessage(
            "previous batch hash mismatch"
        ))
    );

    // 6. Valid next contiguous batch (starts at 4, ends at 4, prev_hash matches batch1.events_hash)
    let valid_batch = SyncMessage::new(
        "batch-4-valid".to_string(),
        NodeId::new("leader-a".to_string()).unwrap(),
        4,
        4,
        0,
        Some(batch1.events_hash.clone()),
        vec![b"e4".to_vec()],
    );
    let ack4 = state.apply_sync_message(&valid_batch).unwrap();
    assert!(ack4.accepted);
    assert_eq!(ack4.received, 1);
    assert_eq!(ack4.last_sequence, 4);

    // 7. Matching overlap recovers a sender cursor lost after the previous ack.
    let recovery_overlap = SyncMessage::new(
        "batch-overlap-recovery".to_string(),
        NodeId::new("leader-a".to_string()).unwrap(),
        4,
        5,
        0,
        Some("stale-sender-hash".to_string()),
        vec![b"e4".to_vec(), b"e5".to_vec()],
    );
    let recovery_ack = state.apply_sync_message(&recovery_overlap).unwrap();
    assert_eq!(recovery_ack.received, 1);
    assert_eq!(recovery_ack.skipped, 1);
    assert_eq!(recovery_ack.last_sequence, 5);
}
