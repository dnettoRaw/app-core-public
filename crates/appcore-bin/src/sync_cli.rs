// =============================================================================
//        #######
//     ###       ###     F: sync_cli.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/02 13:08:16 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/26 10:16:57 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Owns sync CLI actions and the local follower receiver.

#[path = "sync_receiver_cli.rs"]
mod sync_receiver_cli;

pub(crate) use sync_receiver_cli::sync_service_if_enabled;

use crate::bootstrap::{bootstrap_runtime, now_ms, BootstrapError};
use crate::runtime_config::RuntimeConfig;
use appcore_core::NodeId;
use appcore_security::{CommandTokenFactory, TokenClaims, DEFAULT_RUNTIME_TOKEN_TTL_MS};
use appcore_sync::{
    discover_dns_sync_peers, FileSyncCheckpointStore, FollowerSyncClient, HttpSyncTransport,
    ReplicationLog, SyncCheckpointStore, SyncMessage, SyncPeerAddress,
};
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Arc;

pub(crate) fn run_sync_with_action(
    config_path: Option<&str>,
    action: Option<&str>,
) -> Result<(), BootstrapError> {
    let app = bootstrap_runtime(config_path)?;
    let config = app.config.clone();
    let action = action.unwrap_or("status");
    if action == "status" {
        if !config.sync_enabled {
            println!("sync disabled");
            return Ok(());
        }
        println!("sync enabled");
        println!("sync role: {}", config.sync_role);
        if config.sync_peers.is_empty() {
            println!("sync peers: []");
        } else {
            println!("sync peers: {}", config.sync_peers.join(","));
        }
        print_dns_status(&config)?;
        let len = match app.replication_log.as_ref() {
            Some(log) => log
                .lock()
                .len()
                .map_err(|_| BootstrapError::Runtime("sync log observation failed".to_string()))?,
            None => 0,
        };
        println!("sync log len: {}", len);
        return Ok(());
    }
    if action == "push" {
        if !config.sync_enabled {
            println!("sync disabled");
            return Ok(());
        }
        if config.sync_role != "leader" {
            return Err(BootstrapError::Cli(
                "sync push requires role=leader".to_string(),
            ));
        }
        push_sync_to_peers(
            &config,
            app.replication_log.as_ref(),
            Some(&app.security_provider),
        )?;
        println!("sync push completed");
        return Ok(());
    }
    Err(BootstrapError::Cli("unknown sync action".to_string()))
}

pub(crate) fn push_sync_to_peers(
    config: &RuntimeConfig,
    replication_log: Option<&Arc<Mutex<Box<dyn ReplicationLog + Send>>>>,
    security_provider: Option<&appcore_security::HashTokenProvider>,
) -> Result<(), BootstrapError> {
    let peers = resolved_sync_peers(config)?;
    push_sync_to_peer_addresses(config, replication_log, security_provider, peers)
}

pub(crate) fn push_sync_to_peer_addresses(
    config: &RuntimeConfig,
    replication_log: Option<&Arc<Mutex<Box<dyn ReplicationLog + Send>>>>,
    security_provider: Option<&appcore_security::HashTokenProvider>,
    peers: Vec<SyncPeerAddress>,
) -> Result<(), BootstrapError> {
    if peers.is_empty() {
        println!("sync push: no peers configured");
        return Ok(());
    }
    let replication_log = replication_log
        .ok_or_else(|| BootstrapError::Runtime("sync replication log unavailable".to_string()))?;
    let node_id = NodeId::new(&config.node_id).map_err(|e| {
        BootstrapError::Runtime(format!("invalid node_id '{}': {:?}", config.node_id, e))
    })?;
    let checkpoint_store = FileSyncCheckpointStore::new(
        PathBuf::from(&config.storage_path).join("sync-outbound-checkpoints.txt"),
    )
    .map_err(|error| BootstrapError::Runtime(format!("sync checkpoint failed: {error:?}")))?;
    let source_identity = config.core_identity()?;
    let sync_token = if config.sync_require_token {
        let provider = security_provider.ok_or_else(|| {
            BootstrapError::Runtime("sync security provider unavailable".to_string())
        })?;
        let claims = TokenClaims {
            issuer: config.token_issuer.clone(),
            audience: config.token_audience.clone(),
            salt: "sync".to_string(),
            ttl_ms: DEFAULT_RUNTIME_TOKEN_TTL_MS,
        };
        let factory = CommandTokenFactory::new(provider, claims);
        Some(
            factory
                .create_v1_for_purpose(
                    "sync",
                    None,
                    Some(config.node_id.as_str()),
                    now_ms(),
                    DEFAULT_RUNTIME_TOKEN_TTL_MS,
                )
                .map_err(|_| BootstrapError::Runtime("sync token generation failed".to_string()))?,
        )
    } else {
        None
    };
    let mut failures = Vec::new();
    for peer in peers {
        let peer_url = peer.url.clone();
        let peer_key = sync_peer_storage_key(&peer);
        let host = peer.host.clone();
        let port = peer.port;
        let transport = if let Some(token) = &sync_token {
            HttpSyncTransport::new(host, port)
                .with_scheme(peer.scheme)
                .with_auth_token(token.clone())
                .with_source_identity(source_identity.clone())
        } else {
            HttpSyncTransport::new(host, port)
                .with_scheme(peer.scheme)
                .with_source_identity(source_identity.clone())
        };
        let outbox_path =
            PathBuf::from(&config.storage_path).join(format!("sync-outbox-{peer_key}.queue"));
        let result = push_sync_to_peer(
            replication_log,
            &checkpoint_store,
            &peer_key,
            &node_id,
            FollowerSyncClient::new(transport).with_file_outbox(outbox_path),
        );
        if let Err(error) = result {
            failures.push(format!("{peer_url}: {error}"));
        }
    }
    if !failures.is_empty() {
        return Err(BootstrapError::Runtime(format!(
            "sync push failed for {} peer(s): {}",
            failures.len(),
            failures.join("; ")
        )));
    }
    Ok(())
}

fn push_sync_to_peer(
    replication_log: &Arc<Mutex<Box<dyn ReplicationLog + Send>>>,
    checkpoint_store: &dyn SyncCheckpointStore,
    peer_key: &str,
    node_id: &NodeId,
    client: Result<FollowerSyncClient, appcore_sync::SyncError>,
) -> Result<(), String> {
    let client = client.map_err(|error| format!("outbox unavailable: {error:?}"))?;
    if client
        .outbox_stats()
        .map_err(|error| format!("outbox read failed: {error:?}"))?
        .pending_messages
        > 0
    {
        if let Some(last_pending) = client
            .flush_pending_with_progress()
            .map_err(|error| format!("pending retry failed: {error:?}"))?
        {
            checkpoint_store
                .set_checkpoint(
                    peer_key,
                    last_pending.sequence_end,
                    &last_pending.events_hash,
                )
                .map_err(|error| format!("checkpoint update failed: {error:?}"))?;
        }
    }

    let checkpoint = checkpoint_store
        .get_checkpoint(peer_key)
        .map_err(|error| format!("checkpoint read failed: {error:?}"))?;
    let (last_sequence, previous_hash) = checkpoint.unwrap_or((0, String::new()));
    let events = replication_log
        .lock()
        .events_since(last_sequence as usize)
        .map_err(|error| format!("replication log read failed: {error:?}"))?;
    if events.is_empty() {
        return Ok(());
    }
    let sequence_start = last_sequence.saturating_add(1);
    let sequence_end = last_sequence.saturating_add(events.len() as u64);
    let message = SyncMessage::new(
        format!("batch-{}-{}", node_id.as_str(), now_ms()),
        node_id.clone(),
        sequence_start,
        sequence_end,
        now_ms(),
        (!previous_hash.is_empty()).then_some(previous_hash),
        events,
    );
    client
        .push_events(&message)
        .map_err(|error| format!("transport failed: {error:?}"))?;
    checkpoint_store
        .set_checkpoint(peer_key, sequence_end, &message.events_hash)
        .map_err(|error| format!("checkpoint update failed: {error:?}"))
}

fn sync_peer_storage_key(peer: &SyncPeerAddress) -> String {
    let digest = Sha256::digest(peer.url.as_bytes());
    let mut key = String::from("peer-");
    for byte in &digest[..12] {
        key.push_str(&format!("{byte:02x}"));
    }
    key
}

fn print_dns_status(config: &RuntimeConfig) -> Result<(), BootstrapError> {
    println!("sync dns enabled: {}", config.sync_dns_enabled);
    if !config.sync_dns_enabled {
        return Ok(());
    }
    println!("sync dns seeds: {}", config.sync_dns_seeds.join(","));
    let discovered = discover_configured_dns_peers(config)?;
    let urls = discovered
        .iter()
        .map(|peer| peer.url.as_str())
        .collect::<Vec<_>>();
    println!("sync dns resolved: {}", urls.join(","));
    Ok(())
}

fn resolved_sync_peers(config: &RuntimeConfig) -> Result<Vec<SyncPeerAddress>, BootstrapError> {
    let mut peers = parse_static_sync_peers(&config.sync_peers)?;
    peers.extend(discover_configured_dns_peers(config)?);
    peers.sort();
    peers.dedup();
    Ok(peers)
}

pub(crate) fn discovered_sync_peers(
    directory: Option<&appcore_control_plane::PeerDirectory>,
) -> Result<Vec<SyncPeerAddress>, BootstrapError> {
    let Some(directory) = directory else {
        return Ok(Vec::new());
    };
    let mut peers = Vec::new();
    for peer in &directory.peers {
        if !peer.healthy {
            continue;
        }
        for endpoint in &peer.endpoints {
            if endpoint.name == "sync" || endpoint.protocol == "appcore-sync-v1" {
                peers.push(SyncPeerAddress::parse(&endpoint.url).map_err(|_| {
                    BootstrapError::Runtime("invalid discovered sync endpoint".to_string())
                })?);
            }
        }
    }
    peers.sort();
    peers.dedup();
    Ok(peers)
}

fn parse_static_sync_peers(peers: &[String]) -> Result<Vec<SyncPeerAddress>, BootstrapError> {
    peers
        .iter()
        .map(|peer| {
            SyncPeerAddress::parse(peer)
                .map_err(|_| BootstrapError::Cli("invalid sync peer".to_string()))
        })
        .collect()
}

fn discover_configured_dns_peers(
    config: &RuntimeConfig,
) -> Result<Vec<SyncPeerAddress>, BootstrapError> {
    if !config.sync_dns_enabled || config.sync_dns_seeds.is_empty() {
        return Ok(Vec::new());
    }
    discover_dns_sync_peers(&config.sync_dns_seeds, config.sync_dns_default_port)
        .map_err(|err| BootstrapError::Runtime(format!("sync dns discovery failed: {err:?}")))
}
