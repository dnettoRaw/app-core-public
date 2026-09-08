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
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const STATE_FORMAT_VERSION: u16 = 1;
const STATE_FILE: &str = "control-plane-state-v1.json";
const LOCK_FILE: &str = "control-plane-state.lock";
const MAX_CONTROL_PLANE_STATE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_DECODED_CONTROL_PLANE_STATE_BYTES: usize = 64 * 1024 * 1024;
const MAX_DECODED_CONTROL_PLANE_RECORDS: usize = 262_144;
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
        copy_validated_atomic(
            &self.state_path,
            destination.as_ref(),
            "control-plane backup read",
        )
    }

    /// Replaces state from a validated backup.
    pub fn restore_from(&self, source: impl AsRef<Path>) -> ControlPlaneResult<()> {
        let _lock = self.lock_exclusive()?;
        copy_validated_atomic(
            source.as_ref(),
            &self.state_path,
            "control-plane backup read",
        )
    }

    fn initialize(&self) -> ControlPlaneResult<()> {
        let _lock = self.lock_exclusive()?;
        if self.state_path.exists() {
            let _ = self.load_envelope()?;
            return Ok(());
        }
        self.save_control(InMemoryControlPlane::default())
    }

    fn load_control(&self) -> ControlPlaneResult<InMemoryControlPlane> {
        let envelope = self.load_envelope()?;
        InMemoryControlPlane::from_state_with_limits(
            envelope.state,
            MAX_DECODED_CONTROL_PLANE_RECORDS,
            MAX_DECODED_CONTROL_PLANE_STATE_BYTES,
        )
    }

    fn load_envelope(&self) -> ControlPlaneResult<StateEnvelope> {
        decode_envelope_path(&self.state_path, "control-plane state read")
    }

    fn save_control(&self, control: InMemoryControlPlane) -> ControlPlaneResult<()> {
        let envelope = StateEnvelope {
            format_version: STATE_FORMAT_VERSION,
            state: control.into_state()?,
        };
        write_serialized_atomic(&self.state_path, &envelope)
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
            self.save_control(control)?;
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
            self.save_control(control)?;
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
            self.save_control(control)?;
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
            self.save_control(control)?;
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
            self.save_control(control)
        })
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StateEnvelope {
    format_version: u16,
    state: InMemoryState,
}

fn decode_envelope_path(path: &Path, operation: &str) -> ControlPlaneResult<StateEnvelope> {
    reject_symlink(path)?;
    let file = File::open(path).map_err(|error| transport_error(operation, error))?;
    reject_oversized_file(&file)?;
    let mut reader = BufReader::new(file).take(MAX_CONTROL_PLANE_STATE_BYTES.saturating_add(1));
    let result = {
        let mut deserializer = serde_json::Deserializer::from_reader(&mut reader);
        let envelope = StateEnvelope::deserialize(&mut deserializer);
        envelope.and_then(|envelope| {
            deserializer.end()?;
            Ok(envelope)
        })
    };
    let consumed = MAX_CONTROL_PLANE_STATE_BYTES
        .saturating_add(1)
        .saturating_sub(reader.limit());
    if consumed > MAX_CONTROL_PLANE_STATE_BYTES {
        return Err(state_limit_error());
    }
    let envelope = result.map_err(|error| decode_error(operation, error))?;
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

fn reject_oversized_file(file: &File) -> ControlPlaneResult<()> {
    if file
        .metadata()
        .map_err(|error| transport_error("control-plane state metadata", error))?
        .len()
        > MAX_CONTROL_PLANE_STATE_BYTES
    {
        Err(state_limit_error())
    } else {
        Ok(())
    }
}

fn write_serialized_atomic(path: &Path, envelope: &StateEnvelope) -> ControlPlaneResult<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| transport_error("control-plane parent create", error))?;
    let temp = temporary_path(parent);
    let result =
        write_serialized_temp(&temp, envelope).and_then(|()| replace_temp(&temp, path, parent));
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn copy_validated_atomic(
    source: &Path,
    destination: &Path,
    operation: &str,
) -> ControlPlaneResult<()> {
    reject_symlink(source)?;
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| transport_error("control-plane parent create", error))?;
    let temp = temporary_path(parent);
    let result = copy_bounded_to_temp(source, &temp, operation)
        .and_then(|()| decode_envelope_path(&temp, operation).map(drop))
        .and_then(|()| replace_temp(&temp, destination, parent));
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn temporary_path(parent: &Path) -> PathBuf {
    parent.join(format!(
        ".control-plane.{}.{}.tmp",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

fn open_temporary(path: &Path) -> ControlPlaneResult<File> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .map_err(|error| transport_error("control-plane temp create", error))
}

fn write_serialized_temp(temp: &Path, envelope: &StateEnvelope) -> ControlPlaneResult<()> {
    let mut file = open_temporary(temp)?;
    let mut writer = BoundedWriter::new(&mut file, MAX_CONTROL_PLANE_STATE_BYTES);
    let result = serde_json::to_writer(&mut writer, envelope);
    if writer.exceeded {
        return Err(state_limit_error());
    }
    result.map_err(serialization_error)?;
    file.sync_all()
        .map_err(|error| transport_error("control-plane state write", error))
}

fn copy_bounded_to_temp(source: &Path, temp: &Path, operation: &str) -> ControlPlaneResult<()> {
    let mut source = File::open(source).map_err(|error| transport_error(operation, error))?;
    reject_oversized_file(&source)?;
    let mut limited = Read::by_ref(&mut source).take(MAX_CONTROL_PLANE_STATE_BYTES + 1);
    let mut destination = open_temporary(temp)?;
    let copied = std::io::copy(&mut limited, &mut destination)
        .map_err(|error| transport_error(operation, error))?;
    if copied > MAX_CONTROL_PLANE_STATE_BYTES {
        return Err(state_limit_error());
    }
    destination
        .sync_all()
        .map_err(|error| transport_error("control-plane state write", error))
}

fn replace_temp(temp: &Path, path: &Path, _parent: &Path) -> ControlPlaneResult<()> {
    fs::rename(temp, path)
        .map_err(|error| transport_error("control-plane state replace", error))?;
    #[cfg(unix)]
    File::open(_parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| transport_error("control-plane directory sync", error))?;
    Ok(())
}

struct BoundedWriter<W> {
    inner: W,
    remaining: u64,
    exceeded: bool,
}

impl<W> BoundedWriter<W> {
    const fn new(inner: W, limit: u64) -> Self {
        Self {
            inner,
            remaining: limit,
            exceeded: false,
        }
    }
}

impl<W: Write> Write for BoundedWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() as u64 > self.remaining {
            self.exceeded = true;
            return Err(std::io::Error::other(
                "control-plane state exceeds configured limit",
            ));
        }
        let written = self.inner.write(bytes)?;
        self.remaining = self.remaining.saturating_sub(written as u64);
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

fn decode_error(operation: &str, error: serde_json::Error) -> ControlPlaneError {
    if error.is_io() {
        ControlPlaneError::Transport(format!("{operation}: {error}"))
    } else {
        ControlPlaneError::InvalidResponse("NO MORE SUPPORTED PLEASE UPDATE".to_string())
    }
}

fn serialization_error(error: serde_json::Error) -> ControlPlaneError {
    if error.is_io() {
        ControlPlaneError::Transport(format!("control-plane state write: {error}"))
    } else {
        ControlPlaneError::InvalidResponse(error.to_string())
    }
}

fn state_limit_error() -> ControlPlaneError {
    ControlPlaneError::Rejected("control-plane state exceeds configured limit".to_string())
}

fn transport_error(operation: &str, error: std::io::Error) -> ControlPlaneError {
    ControlPlaneError::Transport(format!("{operation}: {error}"))
}
