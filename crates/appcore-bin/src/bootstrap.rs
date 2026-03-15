// =============================================================================
//        #######
//     ###       ###     F: bootstrap.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/02 13:08:16 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Owns runtime bootstrap composition for appcore-bin.

use crate::capability_policy::RuntimeCapabilityPolicy;
use crate::runtime_config::{RuntimeConfig, RuntimeConfigError};
use appcore_api::ApiRouter;
use appcore_contracts::{
    ApplicationManifestV1, DeploymentManifestV1, RuntimeHealth, RuntimeHealthStatus,
    RuntimeManifestV1,
};
use appcore_control_plane::{PeerDirectory, ServiceLeaderLease};
use appcore_core::{
    AppFamily, AppId, AppPlugin, CommandBus, CommandEnvelope, CommandHandler, CommandName,
    CommandRegistry, CommandResult, CoreIdentity, DecisionEngine, DecisionNode, DecisionOutcome,
    DecisionRegistry, DistributedCoreManifest, EventEnvelope, EventName, EventRegistry,
    FileIdempotencyStore, FileOperationalJournal, NodeId, RuntimeBuilder, RuntimeContext,
    RuntimeContractVersion, RuntimeController, RuntimeIdentity, RuntimeLifecycleEvent,
    RuntimeLifecycleState, RuntimeOperationalMode, RuntimeResult, StateRegistry, SyncGroup,
};
use appcore_ops::{
    BasicHealthCheck, FileObservationSink, FileObservationSinkConfig, HealthCheck, HealthReport,
    HealthStatus, InMemoryMetrics, InMemoryObservationSink, ObservationEvent, ObservationKind,
    ObservationMetricsSink, ObservationSeverity, ObservationSink, StaticHeartbeatSource,
};
use appcore_provider::{DeploymentProviderPlan, SecretProvider, SharedCoordinationStore};
use appcore_security::{
    parse_secret_material, HashTokenProvider, SecuritySecretStatus, TokenClaims, TokenProvider,
};
use appcore_storage::{FileStorageProvider, StorageError, StorageProvider, StorageStatus};
use appcore_sync::{FileReplicationLog, FileSyncCheckpointStore, ReplicationLog};
use appcore_update::{FileArtifactStore, SharedUpdateProvider};
use parking_lot::Mutex;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub enum BootstrapError {
    Config(RuntimeConfigError),
    Storage(StorageError),
    Runtime(String),
    Cli(String),
    Exit { code: u8, message: String },
}

impl fmt::Display for BootstrapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BootstrapError::Config(error) => write!(f, "{error}"),
            BootstrapError::Storage(error) => write!(f, "storage error: {error:?}"),
            BootstrapError::Runtime(msg) => write!(f, "{msg}"),
            BootstrapError::Cli(msg) => write!(f, "{msg}"),
            BootstrapError::Exit { message, .. } => write!(f, "{message}"),
        }
    }
}

impl BootstrapError {
    /// Returns the process exit code associated with this controlled failure.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Exit { code, .. } => *code,
            Self::Config(_) => 3,
            Self::Storage(_) | Self::Runtime(_) | Self::Cli(_) => 1,
        }
    }
}

impl From<RuntimeConfigError> for BootstrapError {
    fn from(value: RuntimeConfigError) -> Self {
        BootstrapError::Config(value)
    }
}

impl From<StorageError> for BootstrapError {
    fn from(value: StorageError) -> Self {
        BootstrapError::Storage(value)
    }
}

pub struct BootstrapResult {
    pub(crate) config: RuntimeConfig,
    pub application_manifest: ApplicationManifestV1,
    pub deployment_manifest: DeploymentManifestV1,
    pub provider_plan: DeploymentProviderPlan,
    pub coordination_store: Option<SharedCoordinationStore>,
    pub update_provider: Option<SharedUpdateProvider>,
    pub update_store: FileArtifactStore,
    pub runtime_manifest: RuntimeManifestV1,
    pub observations: InMemoryObservationSink,
    pub metrics: Arc<InMemoryMetrics>,
    pub observation_file_sink: FileObservationSink,
    pub core_identity: CoreIdentity,
    pub core_manifest: DistributedCoreManifest,
    /// Validated Gateway configuration selected by the deployment, when any.
    pub gateway_config: Option<appcore_gateway::GatewayConfig>,
    pub(crate) capability_policy: Arc<RuntimeCapabilityPolicy>,
    pub operation_mode: Arc<Mutex<RuntimeOperationalMode>>,
    pub peer_directory: Arc<Mutex<Option<PeerDirectory>>>,
    pub leader_lease: Arc<Mutex<Option<ServiceLeaderLease>>>,
    pub app_query_router: Option<Arc<Mutex<ApiRouter>>>,
    pub controller: Arc<Mutex<RuntimeController>>,
    pub storage_provider: FileStorageProvider,
    pub health_check: BasicHealthCheck,
    pub heartbeat_source: StaticHeartbeatSource,
    pub replication_log: Option<Arc<Mutex<Box<dyn ReplicationLog + Send>>>>,
    pub replication_log_path: Option<PathBuf>,
    pub checkpoint_store: Option<Arc<FileSyncCheckpointStore>>,
    pub checkpoint_path: Option<PathBuf>,
    pub security_provider: HashTokenProvider,
    pub security_ok: bool,
    pub security_warning: Option<String>,
}

pub(crate) struct LoadedSecurity {
    pub(crate) provider: HashTokenProvider,
    pub(crate) warning: Option<String>,
}

struct BootstrapPlugin {
    application_manifest: ApplicationManifestV1,
    app_id: AppId,
    app_family: AppFamily,
    sync_group: SyncGroup,
    ping_command: CommandName,
    pong_event: EventName,
}

impl BootstrapPlugin {
    fn new(
        config: RuntimeConfig,
        application_manifest: ApplicationManifestV1,
    ) -> Result<Self, BootstrapError> {
        Ok(Self {
            app_id: AppId::new(&config.app_id).map_err(invalid_bootstrap_identity)?,
            app_family: AppFamily::new(&config.app_family).map_err(invalid_bootstrap_identity)?,
            sync_group: SyncGroup::new(&config.sync_group).map_err(invalid_bootstrap_identity)?,
            ping_command: CommandName::new("runtime.ping").map_err(invalid_bootstrap_identity)?,
            pong_event: EventName::new("runtime.pong").map_err(invalid_bootstrap_identity)?,
            application_manifest,
        })
    }
}

struct PingHandler {
    command_name: CommandName,
    event_name: EventName,
}

impl CommandHandler for PingHandler {
    fn command_name(&self) -> CommandName {
        self.command_name.clone()
    }

    fn handle(
        &self,
        command: &CommandEnvelope,
        _context: &dyn RuntimeContext,
    ) -> RuntimeResult<CommandResult> {
        let event = EventEnvelope::new(
            self.event_name.clone(),
            format!("evt-{}", command.command_id),
            command.app_id.clone(),
            command.node_id.clone(),
            command.issued_at_ms,
            command.payload.clone(),
        )?;
        Ok(CommandResult::accepted(vec![event]))
    }
}

struct AllowCommandDecision;
impl DecisionNode for AllowCommandDecision {
    fn name(&self) -> &str {
        "allow.runtime.command"
    }

    fn decide(
        &self,
        _command: &CommandEnvelope,
        _context: &dyn RuntimeContext,
    ) -> RuntimeResult<DecisionOutcome> {
        Ok(DecisionOutcome::Allow)
    }
}

impl AppPlugin for BootstrapPlugin {
    fn application_manifest(&self) -> ApplicationManifestV1 {
        self.application_manifest.clone()
    }

    fn identity(&self, node_id: NodeId) -> RuntimeIdentity {
        RuntimeIdentity {
            app_id: self.app_id.clone(),
            app_family: self.app_family.clone(),
            sync_group: self.sync_group.clone(),
            runtime_contract: RuntimeContractVersion::new(1),
            node_id,
        }
    }

    fn register_commands(&self, registry: &mut CommandRegistry) -> RuntimeResult<()> {
        registry.register(self.ping_command.clone())
    }

    fn register_events(&self, registry: &mut EventRegistry) -> RuntimeResult<()> {
        registry.register(self.pong_event.clone())
    }

    fn register_states(&self, _registry: &mut StateRegistry) -> RuntimeResult<()> {
        Ok(())
    }

    fn register_decisions(&self, registry: &mut DecisionRegistry) -> RuntimeResult<()> {
        registry.register(&AllowCommandDecision)
    }

    fn register_decision_nodes(&self, engine: &mut DecisionEngine) -> RuntimeResult<()> {
        engine.register_node(AllowCommandDecision)
    }

    fn register_handlers(&self, bus: &mut CommandBus) -> RuntimeResult<()> {
        bus.register_handler(PingHandler {
            command_name: self.ping_command.clone(),
            event_name: self.pong_event.clone(),
        })
    }
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub(crate) fn load_config(path: Option<&str>) -> Result<RuntimeConfig, BootstrapError> {
    crate::manifest_bootstrap::load_manifest_input_for_deployment(path).map(|input| input.config)
}

fn prepare_storage(config: &RuntimeConfig) -> Result<FileStorageProvider, BootstrapError> {
    let storage = FileStorageProvider::new(&config.storage_path, &config.backup_path);
    storage.create_dirs()?;
    if storage.health().status == StorageStatus::Offline {
        return Err(BootstrapError::Runtime("storage unavailable".to_string()));
    }
    Ok(storage)
}

fn verify_security(
    config: &RuntimeConfig,
    provider: &HashTokenProvider,
) -> Result<bool, BootstrapError> {
    let payload = b"bootstrap-check";
    let claims = TokenClaims {
        issuer: config.token_issuer.clone(),
        audience: config.token_audience.clone(),
        salt: "bootstrap".to_string(),
        ttl_ms: 1_000,
    };
    let token = provider
        .sign(payload, &claims)
        .map_err(|_| BootstrapError::Runtime("security signing failed".to_string()))?;
    provider
        .verify(payload, &token, &claims)
        .map_err(|_| BootstrapError::Runtime("security verification failed".to_string()))?;
    Ok(true)
}

fn build_controller(
    config: &RuntimeConfig,
    application_manifest: &ApplicationManifestV1,
    app_plugin: Option<&dyn AppPlugin>,
) -> Result<RuntimeController, BootstrapError> {
    let node_id = NodeId::new(&config.node_id).map_err(|e| {
        BootstrapError::Runtime(format!("invalid node_id '{}': {:?}", config.node_id, e))
    })?;
    let plugin = BootstrapPlugin::new(config.clone(), application_manifest.clone())?;
    let mut builder = RuntimeBuilder::new();
    if let Some(custom) = app_plugin {
        builder.with_plugin(custom, node_id.clone()).map_err(|e| {
            BootstrapError::Runtime(format!("custom plugin registration failed: {e:?}"))
        })?;
        builder.with_additional_plugin(&plugin).map_err(|e| {
            BootstrapError::Runtime(format!("bootstrap plugin registration failed: {e:?}"))
        })?;
    } else {
        builder.with_plugin(&plugin, node_id).map_err(|e| {
            BootstrapError::Runtime(format!("bootstrap plugin registration failed: {e:?}"))
        })?;
    }
    let instance = builder
        .build()
        .map_err(|_| BootstrapError::Runtime("runtime build failed".to_string()))?;
    let journal_path = PathBuf::from(&config.storage_path).join("operational-journal.jsonl");
    let journal = Arc::new(
        FileOperationalJournal::open(journal_path, 20_000, 64 * 1024 * 1024).map_err(|_| {
            BootstrapError::Runtime("failed to init operational journal".to_string())
        })?,
    );
    instance.audit_log().attach_journal(Arc::clone(&journal));
    instance.event_bus().attach_journal(journal);
    let idempotency_path = PathBuf::from(&config.storage_path).join("idempotency.txt");
    let idempotency_store =
        FileIdempotencyStore::new_with_ttl(&idempotency_path, Some(config.idempotency_ttl_ms))
            .map_err(|_| BootstrapError::Runtime("failed to init idempotency store".to_string()))?;
    let mut controller =
        RuntimeController::with_idempotency_store(instance, Box::new(idempotency_store));
    set_running_lifecycle(&mut controller)?;
    Ok(controller)
}

fn invalid_bootstrap_identity(error: impl std::fmt::Debug) -> BootstrapError {
    BootstrapError::Runtime(format!("invalid bootstrap identity: {error:?}"))
}

fn set_running_lifecycle(controller: &mut RuntimeController) -> Result<(), BootstrapError> {
    for event in [
        RuntimeLifecycleEvent::ConfigLoaded,
        RuntimeLifecycleEvent::SecurityChecked,
        RuntimeLifecycleEvent::StorageOpened,
        RuntimeLifecycleEvent::ApiStarted,
    ] {
        controller
            .apply_lifecycle_event(event)
            .map_err(|_| BootstrapError::Runtime("lifecycle transition failed".to_string()))?;
    }
    Ok(())
}

pub(crate) fn load_security_provider(
    config: &RuntimeConfig,
) -> Result<LoadedSecurity, BootstrapError> {
    load_security_provider_with(
        config,
        &crate::providers::DeploymentSecretResolver::default(),
    )
}

pub(crate) fn load_security_provider_with(
    config: &RuntimeConfig,
    secrets: &dyn SecretProvider,
) -> Result<LoadedSecurity, BootstrapError> {
    if config.security_provider != "hashtoken" {
        return Err(BootstrapError::Runtime(
            "unsupported security provider".to_string(),
        ));
    }
    let reference = security_reference(config)?;
    load_security_provider_from_reference(config, secrets, &reference)
}

pub(crate) fn load_deployment_security_provider(
    config: &RuntimeConfig,
    secrets: &crate::providers::DeploymentSecretResolver,
) -> Result<LoadedSecurity, BootstrapError> {
    let reference = security_reference(config)?;
    if let Some(provider) =
        secrets.rotating_hash_token_provider(&reference, security_salts(config))?
    {
        return Ok(LoadedSecurity {
            provider,
            warning: None,
        });
    }
    load_security_provider_from_reference(config, secrets, &reference)
}

fn security_reference(
    config: &RuntimeConfig,
) -> Result<appcore_contracts::SecretRef, BootstrapError> {
    if let Some(environment_key) = &config.security_secret_env {
        appcore_contracts::SecretRef::new(format!("env:{environment_key}"))
    } else if config.security_secret_path.starts_with("provider:") {
        appcore_contracts::SecretRef::new(config.security_secret_path.clone())
    } else {
        appcore_contracts::SecretRef::new(format!("file:{}", config.security_secret_path))
    }
    .map_err(|error| BootstrapError::Runtime(format!("invalid security secret ref: {error}")))
}

pub(crate) fn load_security_provider_from_reference(
    config: &RuntimeConfig,
    secrets: &dyn SecretProvider,
    reference: &appcore_contracts::SecretRef,
) -> Result<LoadedSecurity, BootstrapError> {
    let raw = secrets.resolve(reference).map_err(|error| {
        BootstrapError::Runtime(format!("security secret load failed: {error}"))
    })?;
    let material = parse_secret_material(raw.expose().as_bytes())
        .map_err(|_| BootstrapError::Runtime("security secret format invalid".to_string()))?;
    if material.secret.len() < 16 {
        return Err(BootstrapError::Runtime(
            "security secret too short".to_string(),
        ));
    }
    if material.metadata.status == SecuritySecretStatus::Revoked {
        return Err(BootstrapError::Runtime(
            "security secret revoked".to_string(),
        ));
    }
    let expired = material.is_expired(now_ms());
    if expired && !config.security_allow_expired_secret {
        return Err(BootstrapError::Runtime(
            "security secret expired".to_string(),
        ));
    }
    let warning = if material.metadata.status == SecuritySecretStatus::Deprecated {
        Some(format!(
            "security secret key_id={} status=deprecated",
            material.metadata.key_id
        ))
    } else if expired {
        Some(format!(
            "security secret key_id={} is expired but allowed",
            material.metadata.key_id
        ))
    } else {
        None
    };
    let provider = HashTokenProvider::with_secret(material.secret.clone(), security_salts(config))
        .map_err(|_| BootstrapError::Runtime("security token material invalid".to_string()))?;
    Ok(LoadedSecurity { provider, warning })
}

fn security_salts(config: &RuntimeConfig) -> Vec<Vec<u8>> {
    vec![
        config.app_id.as_bytes().to_vec(),
        config.sync_group.as_bytes().to_vec(),
    ]
}

mod assembly;
mod observability;

pub(crate) use assembly::bootstrap_runtime_from_manifest;
pub use assembly::{bootstrap_runtime, bootstrap_runtime_with_plugin};
