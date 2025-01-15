// =============================================================================
//        #######
//     ###       ###     F: file.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 14:12:17 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Crash-consistent file-backed reference control plane.

use super::memory::{InMemoryControlPlane, InMemoryState};
use super::*;
use appcore_core::{Clock, SystemClock};
use fs2::FileExt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const STATE_FORMAT_VERSION: u16 = 1;
const STATE_FILE: &str = "control-plane-state-v1.json";
const LOCK_FILE: &str = "control-plane-state.lock";
const MAX_CONTROL_PLANE_STATE_BYTES: u64 = 16 * 1024 * 1024;
// appcore-norm: allow(global-state) reason: atomic sequence prevents process-local temporary path collisions
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Durable control-plane implementation for one shared deployment directory.
///
/// Every operation takes an operating-system file lock, reloads validated
/// state, applies one contract operation, and atomically persists the result.
/// The deployment directory is the authentication and isolation boundary and
/// is created with owner-only permissions on Unix.
#[derive(Clone)]
pub struct FileControlPlane {
    root: PathBuf,
    state_path: PathBuf,
    lock_path: PathBuf,
    retention_ms: u64,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for FileControlPlane {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FileControlPlane")
            .field("root", &self.root)
            .field("retention_ms", &self.retention_ms)
            .finish_non_exhaustive()
    }
}

impl FileControlPlane {
    /// Opens or creates a durable reference control plane.
    pub fn open(root: impl Into<PathBuf>, retention_ms: u64) -> ControlPlaneResult<Self> {
        Self::with_clock(root, retention_ms, Arc::new(SystemClock::new()))
    }

    /// Opens a control plane using an explicit authoritative server clock.
    pub fn with_clock(
        root: impl Into<PathBuf>,
        retention_ms: u64,
        clock: Arc<dyn Clock>,
    ) -> ControlPlaneResult<Self> {
        if retention_ms == 0 {
            return Err(ControlPlaneError::Rejected(
                "control-plane retention must be greater than zero".to_string(),
            ));
        }
        let root = root.into();
        prepare_root(&root)?;
        let control = Self {
            state_path: root.join(STATE_FILE),
            lock_path: root.join(LOCK_FILE),
            root,
            retention_ms,
            clock,
        };
        control.initialize()?;
        Ok(control)
    }

    /// Returns the durable state path.
    pub fn state_path(&self) -> &Path {
        &self.state_path
    }

    /// Returns the presence retention window in milliseconds.
    pub fn retention_ms(&self) -> u64 {
        self.retention_ms
    }

    /// Creates an integrity-validated point-in-time state backup.
    pub fn backup_to(&self, destination: impl AsRef<Path>) -> ControlPlaneResult<()> {
        let _lock = self.lock_exclusive()?;
        let envelope = self.load_envelope()?;
        let encoded = encode_envelope(&envelope)?;
        write_atomic(destination.as_ref(), &encoded)
    }

    /// Replaces state from a validated backup.
    pub fn restore_from(&self, source: impl AsRef<Path>) -> ControlPlaneResult<()> {
        reject_symlink(source.as_ref())?;
        let encoded = read_bounded(source.as_ref(), "control-plane backup read")?;
        let _ = decode_envelope(&encoded)?;
        let _lock = self.lock_exclusive()?;
        write_atomic(&self.state_path, &encoded)
    }

    fn initialize(&self) -> ControlPlaneResult<()> {
        let _lock = self.lock_exclusive()?;
        if self.state_path.exists() {
            let _ = self.load_envelope()?;
            return Ok(());
        }
        self.save_control(&InMemoryControlPlane::default())
    }

    fn load_control(&self) -> ControlPlaneResult<InMemoryControlPlane> {
        let envelope = self.load_envelope()?;
        Ok(InMemoryControlPlane::from_state(envelope.state))
    }

    fn load_envelope(&self) -> ControlPlaneResult<StateEnvelope> {
        reject_symlink(&self.state_path)?;
        let encoded = read_bounded(&self.state_path, "control-plane state read")?;
        decode_envelope(&encoded)
    }

    fn save_control(&self, control: &InMemoryControlPlane) -> ControlPlaneResult<()> {
        let envelope = StateEnvelope {
            format_version: STATE_FORMAT_VERSION,
            state: control.snapshot()?,
        };
        write_atomic(&self.state_path, &encode_envelope(&envelope)?)
    }

    fn prune(&self, control: &InMemoryControlPlane, now_ms: u64) -> ControlPlaneResult<()> {
        let cutoff = now_ms.saturating_sub(self.retention_ms);
        let _ = control.prune_registrations(cutoff)?;
        Ok(())
    }

    fn lock_exclusive(&self) -> ControlPlaneResult<File> {
        reject_symlink(&self.lock_path)?;
        let mut options = OpenOptions::new();
        options.create(true).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(&self.lock_path)
            .map_err(|error| transport_error("control-plane lock open", error))?;
        file.lock_exclusive()
            .map_err(|error| transport_error("control-plane lock acquire", error))?;
        Ok(file)
    }
}

impl ControlPlaneProvider for FileControlPlane {
    fn register<'a>(
        &'a self,
        mut registration: CoreRegistration,
    ) -> ControlPlaneFuture<'a, CorePresence> {
        Box::pin(async move {
            let _lock = self.lock_exclusive()?;
            let control = self.load_control()?;
            let now_ms = self.clock.now_ms();
            registration.registered_at_ms = now_ms;
            self.prune(&control, now_ms)?;
            let result = control.register(registration).await?;
            self.save_control(&control)?;
            Ok(result)
        })
    }

    fn heartbeat<'a>(
        &'a self,
        mut request: HeartbeatRequest,
    ) -> ControlPlaneFuture<'a, HeartbeatResponse> {
        Box::pin(async move {
            let _lock = self.lock_exclusive()?;
            let control = self.load_control()?;
            let now_ms = self.clock.now_ms();
            request.sent_at_ms = now_ms;
            self.prune(&control, now_ms)?;
            let result = control.heartbeat(request).await?;
            self.save_control(&control)?;
            Ok(result)
        })
    }

    fn discover_peers<'a>(
        &'a self,
        identity: &'a CoreIdentity,
    ) -> ControlPlaneFuture<'a, PeerDirectory> {
        Box::pin(async move {
            let _lock = self.lock_exclusive()?;
            let control = self.load_control()?;
            let now_ms = self.clock.now_ms();
            self.prune(&control, now_ms)?;
            let mut result = control.discover_peers(identity).await?;
            result.refreshed_at_ms = now_ms;
            self.save_control(&control)?;
            Ok(result)
        })
    }

    fn acquire_or_renew_service_lease<'a>(
        &'a self,
        identity: &'a CoreIdentity,
        service_id: &'a ServiceId,
        ttl_ms: u64,
        _client_now_ms: u64,
    ) -> ControlPlaneFuture<'a, ServiceLeaderLease> {
        Box::pin(async move {
            let _lock = self.lock_exclusive()?;
            let control = self.load_control()?;
            let result = control
                .acquire_or_renew_service_lease(identity, service_id, ttl_ms, self.clock.now_ms())
                .await?;
            self.save_control(&control)?;
            Ok(result)
        })
    }

    fn release_service_lease<'a>(
        &'a self,
        lease: ServiceLeaderLease,
    ) -> ControlPlaneFuture<'a, ()> {
        Box::pin(async move {
            let _lock = self.lock_exclusive()?;
            let control = self.load_control()?;
            control.release_service_lease(lease).await?;
            self.save_control(&control)
        })
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StateEnvelope {
    format_version: u16,
    state: InMemoryState,
}

fn encode_envelope(envelope: &StateEnvelope) -> ControlPlaneResult<Vec<u8>> {
    let encoded = serde_json::to_vec(envelope)
        .map_err(|error| ControlPlaneError::InvalidResponse(error.to_string()))?;
    if encoded.len() as u64 > MAX_CONTROL_PLANE_STATE_BYTES {
        return Err(ControlPlaneError::Rejected(
            "control-plane state exceeds configured limit".to_string(),
        ));
    }
    Ok(encoded)
}

fn decode_envelope(encoded: &[u8]) -> ControlPlaneResult<StateEnvelope> {
    let envelope = serde_json::from_slice::<StateEnvelope>(encoded).map_err(|_| {
        ControlPlaneError::InvalidResponse("NO MORE SUPPORTED PLEASE UPDATE".to_string())
    })?;
    if envelope.format_version != STATE_FORMAT_VERSION {
        return Err(ControlPlaneError::InvalidResponse(
            "NO MORE SUPPORTED PLEASE UPDATE".to_string(),
        ));
    }
    Ok(envelope)
}

fn prepare_root(root: &Path) -> ControlPlaneResult<()> {
    reject_symlink(root)?;
    fs::create_dir_all(root)
        .map_err(|error| transport_error("control-plane directory create", error))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root, fs::Permissions::from_mode(0o700))
            .map_err(|error| transport_error("control-plane directory permissions", error))?;
    }
    Ok(())
}

fn reject_symlink(path: &Path) -> ControlPlaneResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(ControlPlaneError::Rejected(
            "control-plane path cannot be a symlink".to_string(),
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(transport_error("control-plane path inspection", error)),
    }
}

fn read_bounded(path: &Path, operation: &str) -> ControlPlaneResult<Vec<u8>> {
    reject_symlink(path)?;
    let mut file = File::open(path).map_err(|error| transport_error(operation, error))?;
    let mut encoded = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_CONTROL_PLANE_STATE_BYTES.saturating_add(1))
        .read_to_end(&mut encoded)
        .map_err(|error| transport_error(operation, error))?;
    if encoded.len() as u64 > MAX_CONTROL_PLANE_STATE_BYTES {
        return Err(ControlPlaneError::Rejected(
            "control-plane state exceeds configured limit".to_string(),
        ));
    }
    Ok(encoded)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> ControlPlaneResult<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| transport_error("control-plane parent create", error))?;
    let temp = parent.join(format!(
        ".control-plane.{}.{}.tmp",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let result = write_and_replace(&temp, path, parent, bytes);
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

fn write_and_replace(
    temp: &Path,
    path: &Path,
    _parent: &Path,
    bytes: &[u8],
) -> ControlPlaneResult<()> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(temp)
        .map_err(|error| transport_error("control-plane temp create", error))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| transport_error("control-plane state write", error))?;
    fs::rename(temp, path)
        .map_err(|error| transport_error("control-plane state replace", error))?;
    #[cfg(unix)]
    File::open(_parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| transport_error("control-plane directory sync", error))?;
    Ok(())
}

fn transport_error(operation: &str, error: std::io::Error) -> ControlPlaneError {
    ControlPlaneError::Transport(format!("{operation}: {error}"))
}
