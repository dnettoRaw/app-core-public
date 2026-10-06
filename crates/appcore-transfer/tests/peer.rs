// =============================================================================
//        #######
//     ###       ###     F: peer.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/06 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/06 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Signed/authenticated stream integration with object authorization and bounded registry.
#![cfg(feature = "peer-rpc")]
use appcore_core::*;
use appcore_peer_rpc::{v2::*, *};
use appcore_security::{HashTokenProvider, TokenClaims};
use appcore_transfer::{peer::*, *};
#[cfg(unix)]
use std::os::unix::fs::DirBuilderExt;
use std::{
    fs,
    io::Cursor,
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let mut random = [0; 8];
        getrandom::fill(&mut random).unwrap();
        let path = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "transfer-peer-{}-{}",
                std::process::id(),
                u64::from_le_bytes(random)
            ));
        #[cfg(unix)]
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        #[cfg(not(unix))]
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct Repository {
    descriptor: ObjectDescriptor,
    bytes: Vec<u8>,
    allowed_origin: String,
}
impl TransferRepository for Repository {
    fn open_authorized(
        &self,
        open: &PeerRpcStreamOpenV2,
        hash: &str,
    ) -> TransferResult<(ObjectDescriptor, Box<dyn ObjectReader>)> {
        if open.tenant_id.as_str() != "tenant-a"
            || open.cluster_id.as_str() != "cluster-a"
            || open.source_core_id.as_str() != self.allowed_origin
            || hash != self.descriptor.object_hash()
        {
            return Err(TransferError::Source);
        }
        Ok((
            self.descriptor.clone(),
            Box::new(Cursor::new(self.bytes.clone())),
        ))
    }
}
struct Loopback {
    registry: Arc<PeerRpcStreamRegistry>,
    auth: HashTokenPeerAuthenticator,
    validator: PeerRpcValidator,
}
impl PeerTransportProvider for Loopback {
    fn send(
        &self,
        _base: &str,
        request: PeerRpcHttpRequest,
    ) -> Result<PeerRpcHttpResponse, PeerRpcError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        self.auth.authenticate(
            request.bearer_token.as_deref(),
            Some(&payload_hash(&request.body)),
            now,
        )?;
        if request.path != PEER_QUERY_PATH_V2 {
            return Err(PeerRpcError::EndpointUnavailable);
        }
        let frame: PeerRpcStreamFrameV2 =
            serde_json::from_slice(&request.body).map_err(|_| PeerRpcError::Unauthorized)?;
        if let PeerRpcStreamFrameV2::Open(open) = &frame {
            self.validator.validate_stream_open_v2(open, now)?;
        }
        let reply = self
            .registry
            .exchange(PeerRpcCallKind::Query, frame, now)
            .map_err(|_| PeerRpcError::Forbidden)?;
        Ok(PeerRpcHttpResponse {
            status_code: 200,
            body: serde_json::to_vec(&reply).unwrap(),
        })
    }
}
fn identity() -> CoreIdentity {
    CoreIdentity {
        tenant_id: TenantId::new("tenant-a").unwrap(),
        cluster_id: ClusterId::new("cluster-a").unwrap(),
        core_id: CoreId::new("core-a").unwrap(),
        instance_id: InstanceId::new("instance-a").unwrap(),
        kind: CoreKind::operational(),
        protocol_version: ProtocolVersion::new(1),
        runtime: RuntimeIdentity {
            app_id: AppId::new("transfer-test").unwrap(),
            app_family: AppFamily::new("transfer-test").unwrap(),
            sync_group: SyncGroup::new("cluster-a").unwrap(),
            runtime_contract: RuntimeContractVersion::new(1),
            node_id: NodeId::new("node-a").unwrap(),
        },
    }
}
fn client(
    registry: Arc<PeerRpcStreamRegistry>,
) -> PeerRpcClient<Loopback, HashTokenPeerTokenIssuer> {
    let mut secret = vec![0; 32];
    getrandom::fill(&mut secret).unwrap();
    let provider = HashTokenProvider::from_secret(secret).unwrap();
    let claims = TokenClaims {
        issuer: "transfer-test".into(),
        audience: "transfer-test".into(),
        salt: "peer".into(),
        ttl_ms: 5000,
    };
    let transport = Loopback {
        registry,
        auth: HashTokenPeerAuthenticator::new(provider.clone(), claims.clone()),
        validator: PeerRpcValidator::new(PeerRpcValidationConfig {
            local_tenant_id: TenantId::new("tenant-a").unwrap(),
            local_cluster_id: ClusterId::new("cluster-a").unwrap(),
            local_core_id: CoreId::new("core-b").unwrap(),
            max_payload_bytes: 1024 * 1024,
            nonce_window_ms: 60000,
        }),
    };
    PeerRpcClient::new(
        identity(),
        PeerRpcClientConfig {
            envelope_ttl_ms: 5000,
            request_timeout_ms: 1000,
            ..PeerRpcClientConfig::default()
        },
        transport,
        HashTokenPeerTokenIssuer::new(provider, claims),
    )
}
fn registry(
    spool: &Fixture,
    descriptor: ObjectDescriptor,
    bytes: Vec<u8>,
    origin: &str,
) -> Arc<PeerRpcStreamRegistry> {
    let dispatcher = TransferDispatcher::new(
        Repository {
            descriptor,
            bytes,
            allowed_origin: origin.into(),
        },
        TransferLimits::default(),
    )
    .unwrap();
    let registry = Arc::new(
        PeerRpcStreamRegistry::new(
            PeerRpcStreamRegistryConfig {
                max_sessions: 4,
                max_reserved_payload_bytes: 4 * 1024 * 1024,
                spool_directory: spool.0.clone(),
                chunk_limits: PeerRpcChunkLimits::default(),
            },
            Arc::new(dispatcher),
        )
        .unwrap(),
    );
    registry
        .set_capability_limits(
            CapabilityName::new(TRANSFER_BLOCK_CAPABILITY).unwrap(),
            PeerRpcCapabilityLimits::new(512, 64 * 1024, 5000).unwrap(),
        )
        .unwrap();
    registry
}
#[test]
fn signed_v2_transfer_matches_bytes_and_releases_stream_spool() {
    let spool = Fixture::new();
    let storage = Fixture::new();
    let bytes = vec![42; 3 * 64 * 1024 + 17];
    let descriptor = ObjectDescriptor::from_reader(
        Cursor::new(&bytes),
        bytes.len() as u64,
        64 * 1024,
        TransferLimits::default(),
    )
    .unwrap();
    let registry = registry(&spool, descriptor.clone(), bytes.clone(), "core-a");
    let client = client(registry.clone());
    let source = PeerBlockSource::new(
        &client,
        "https://selected.invalid",
        CoreId::new("core-b").unwrap(),
    )
    .unwrap();
    let sources = [SourceBinding::approved("core-b", &source).unwrap()];
    let mut store = TransferStore::open(&storage.0, TransferLimits::default()).unwrap();
    let result = transfer(
        store.begin(descriptor).unwrap(),
        &sources,
        TransferOptions::default(),
        &TransferControl::new(Duration::from_secs(10)).unwrap(),
    )
    .unwrap();
    assert_eq!(fs::read(result.object.path()).unwrap(), bytes);
    assert_eq!(registry.snapshot().unwrap().active_sessions, 0);
    assert_eq!(fs::read_dir(&spool.0).unwrap().count(), 0);
}
#[test]
fn repository_object_policy_rejection_never_publishes_bytes() {
    let spool = Fixture::new();
    let storage = Fixture::new();
    let bytes = vec![42; 32];
    let descriptor = ObjectDescriptor::from_reader(
        Cursor::new(&bytes),
        bytes.len() as u64,
        32,
        TransferLimits::default(),
    )
    .unwrap();
    let registry = registry(&spool, descriptor.clone(), bytes, "other-origin");
    let client = client(registry.clone());
    let source = PeerBlockSource::new(
        &client,
        "https://selected.invalid",
        CoreId::new("core-b").unwrap(),
    )
    .unwrap();
    let sources = [SourceBinding::approved("core-b", &source).unwrap()];
    let mut store = TransferStore::open(&storage.0, TransferLimits::default()).unwrap();
    let hash = descriptor.object_hash().to_owned();
    let result = transfer(
        store.begin(descriptor).unwrap(),
        &sources,
        TransferOptions {
            max_attempts: 1,
            ..TransferOptions::default()
        },
        &TransferControl::new(Duration::from_secs(10)).unwrap(),
    );
    assert_eq!(result.unwrap_err(), TransferError::SourcesExhausted);
    assert!(!storage.0.join(hash).join("complete.bin").exists());
    assert_eq!(registry.snapshot().unwrap().active_sessions, 0);
}
