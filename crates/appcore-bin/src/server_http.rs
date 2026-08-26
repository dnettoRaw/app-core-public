// =============================================================================
//        #######
//     ###       ###     F: server_http.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/06 20:47:13 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Runtime HTTP host authentication and state wiring.

use super::RuntimeServer;
use crate::bootstrap::{now_ms, BootstrapError};
use crate::runtime_config::RuntimeConfig;
use appcore_api::{
    CommandTokenVerifier, HttpApiConfig, HttpCommandAuth, RuntimeHttpHost, RuntimeHttpStateParts,
    RuntimeStaticInfo, SyncLogView, SyncLogViewError,
};
use appcore_peer_rpc::{FilePeerNonceStore, PeerNonceStore};
use appcore_security::{CommandTokenError, CommandTokenValidator, HashTokenProvider, TokenClaims};
use appcore_storage::StorageProvider;
use appcore_supervisor::ManagedService;
use appcore_sync::ReplicationLog;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;

struct RuntimeSyncLogView {
    replication_log: Arc<Mutex<Box<dyn ReplicationLog + Send>>>,
}

pub(crate) struct RuntimeCommandTokenVerifier {
    pub(crate) provider: HashTokenProvider,
    pub(crate) claims: TokenClaims,
    pub(crate) replay_store: Arc<dyn PeerNonceStore>,
}

impl SyncLogView for RuntimeSyncLogView {
    fn len(&self) -> Result<usize, SyncLogViewError> {
        self.replication_log
            .lock()
            .len()
            .map_err(|_| SyncLogViewError)
    }
}

pub(crate) fn http_service_if_enabled(
    server: &RuntimeServer,
) -> Result<Option<Arc<dyn ManagedService>>, BootstrapError> {
    if !server.app.config.api_enabled {
        return Ok(None);
    }
    let host = Arc::new(build_http_host(server)?);
    let descriptor = crate::runtime_services::service_descriptor(
        crate::runtime_services::HTTP_SERVICE,
        appcore_supervisor::ManagedResource::Http,
        &[crate::runtime_services::SECURITY_SERVICE],
    )?;
    Ok(Some(Arc::new(
        appcore_supervisor::ManagedThreadService::new(descriptor, move |shutdown| {
            let host = Arc::clone(&host);
            std::thread::Builder::new()
                .name("appcore-http".to_string())
                .spawn(move || {
                    host.run_until_shutdown(shutdown)
                        .map_err(|error| format!("http host failed: {error}"))
                })
                .map_err(|error| error.to_string())
        }),
    )))
}

fn build_http_host(server: &RuntimeServer) -> Result<RuntimeHttpHost, BootstrapError> {
    Ok(RuntimeHttpHost::with_state_parts(
        http_api_config(&server.app.config),
        runtime_static_info(server)?,
        RuntimeHttpStateParts {
            controller: Some(server.app.controller.clone()),
            sync_log: sync_log_view(server),
            tick_counter: Some(Arc::clone(&server.tick_counter)),
            operation_mode: Some(Arc::clone(&server.app.operation_mode)),
            command_policy: Some(server.app.capability_policy.clone()),
            supervisor: Some(server.service_supervisor.clone()),
            auth: build_http_auth(server)?,
            app_query_router: server.app.app_query_router.clone(),
        },
    ))
}

fn http_api_config(config: &RuntimeConfig) -> HttpApiConfig {
    HttpApiConfig {
        host: config.api_host.clone(),
        port: config.api_port,
        enabled: config.api_enabled,
        max_payload_bytes: config.api_max_payload_bytes,
    }
}

fn runtime_static_info(server: &RuntimeServer) -> Result<RuntimeStaticInfo, BootstrapError> {
    Ok(RuntimeStaticInfo {
        app_id: server.app.config.app_id.clone(),
        node_id: server.app.config.node_id.clone(),
        tenant_id: server.app.config.tenant_id.clone(),
        cluster_id: server.app.config.cluster_id.clone(),
        core_id: server.app.config.core_id.clone(),
        operation_mode: server.app.operation_mode.lock().as_str().to_string(),
        storage_status: format!("{:?}", server.app.storage_provider.health().status),
        security_ok: server.app.security_ok,
        api_enabled: server.app.config.api_enabled,
        sync_enabled: server.app.config.sync_enabled,
        sync_role: server.app.config.sync_role.clone(),
        sync_log_len: sync_log_len(server)?,
        sync_log_path: server
            .app
            .replication_log_path
            .as_ref()
            .map(|path| path.display().to_string()),
        sync_checkpoint_path: server
            .app
            .checkpoint_path
            .as_ref()
            .map(|path| path.display().to_string()),
        sync_peers: server.app.config.sync_peers.clone(),
        sync_dns_enabled: server.app.config.sync_dns_enabled,
        sync_dns_seeds: server.app.config.sync_dns_seeds.clone(),
        sync_dns_default_port: server.app.config.sync_dns_default_port,
        idempotency_ttl_ms: server.app.config.idempotency_ttl_ms,
        idempotency_path: idempotency_path(server),
    })
}

fn sync_log_view(server: &RuntimeServer) -> Option<Arc<dyn SyncLogView>> {
    server.app.replication_log.as_ref().map(|log| {
        Arc::new(RuntimeSyncLogView {
            replication_log: Arc::clone(log),
        }) as Arc<dyn SyncLogView>
    })
}

fn sync_log_len(server: &RuntimeServer) -> Result<usize, BootstrapError> {
    match server.app.replication_log.as_ref() {
        Some(log) => log
            .lock()
            .len()
            .map_err(|_| BootstrapError::Runtime("sync log observation failed".to_string())),
        None => Ok(0),
    }
}

fn idempotency_path(server: &RuntimeServer) -> Option<String> {
    Some(
        PathBuf::from(&server.app.config.storage_path)
            .join("idempotency.txt")
            .display()
            .to_string(),
    )
}

fn build_http_auth(server: &RuntimeServer) -> Result<HttpCommandAuth, BootstrapError> {
    let config = &server.app.config;
    let verifier = if config.api_require_token || !config.api_public_status {
        let replay_store = FilePeerNonceStore::open(
            PathBuf::from(&config.storage_path).join("security/http-token-jti.json"),
        )
        .map_err(|error| {
            BootstrapError::Runtime(format!("HTTP replay store initialization failed: {error}"))
        })?;
        Some(build_token_verifier(
            config,
            server.app.security_provider.clone(),
            Arc::new(replay_store),
        ))
    } else {
        None
    };
    Ok(HttpCommandAuth {
        require_token: config.api_require_token,
        public_status: config.api_public_status,
        verifier,
    })
}

fn build_token_verifier(
    config: &RuntimeConfig,
    provider: HashTokenProvider,
    replay_store: Arc<dyn PeerNonceStore>,
) -> Arc<dyn CommandTokenVerifier> {
    let claims = TokenClaims {
        issuer: config.token_issuer.clone(),
        audience: config.token_audience.clone(),
        salt: "command".to_string(),
        ttl_ms: 60_000,
    };
    Arc::new(RuntimeCommandTokenVerifier {
        provider,
        claims,
        replay_store,
    })
}

impl CommandTokenVerifier for RuntimeCommandTokenVerifier {
    fn verify_command_token(
        &self,
        token: &str,
        command_name: &str,
    ) -> Result<(), CommandTokenError> {
        self.validator("command").validate_for_purpose(
            token,
            "command",
            Some(command_name),
            now_ms(),
        )
    }

    fn verify_query_token(&self, token: &str, query_name: &str) -> Result<(), CommandTokenError> {
        self.validator("query")
            .validate_for_purpose(token, "query", Some(query_name), now_ms())
    }

    fn verify_command_token_with_request(
        &self,
        token: &str,
        command_name: &str,
        details: Option<&appcore_api::RequestValidationDetails>,
    ) -> Result<(), CommandTokenError> {
        self.validate_with_request(token, "command", command_name, details)
    }

    fn verify_query_token_with_request(
        &self,
        token: &str,
        query_name: &str,
        details: Option<&appcore_api::RequestValidationDetails>,
    ) -> Result<(), CommandTokenError> {
        self.validate_with_request(token, "query", query_name, details)
    }
}

impl RuntimeCommandTokenVerifier {
    fn validator(&self, salt: &str) -> CommandTokenValidator<'_, HashTokenProvider> {
        let mut claims = self.claims.clone();
        claims.salt = salt.to_string();
        CommandTokenValidator::new(&self.provider, claims)
    }

    fn validate_with_request(
        &self,
        token: &str,
        purpose: &str,
        name: &str,
        details: Option<&appcore_api::RequestValidationDetails>,
    ) -> Result<(), CommandTokenError> {
        let hash = details.map(appcore_security::compute_request_hash);
        let claims = self.validator(purpose).validate_and_get_claims(
            token,
            purpose,
            Some(name),
            now_ms(),
            hash.as_deref(),
        )?;
        self.reject_replayed_jti(&claims)
    }

    fn reject_replayed_jti(
        &self,
        claims: &appcore_security::RuntimeTokenClaims,
    ) -> Result<(), CommandTokenError> {
        let Some(jti) = &claims.jti else {
            return Ok(());
        };
        let now = now_ms();
        let key = format!("{}:{}", claims.purpose, jti);
        self.replay_store
            .check_and_record(&key, claims.expires_at_ms, now)
            .map_err(|_| CommandTokenError::Unauthorized)
    }
}
