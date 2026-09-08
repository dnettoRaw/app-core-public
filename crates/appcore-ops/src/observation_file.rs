// =============================================================================
//        #######
//     ###       ###     F: observation_file.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 11:51:10 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Bounded asynchronous JSONL drain for local production operations.

use crate::{ObservationEvent, ObservationSink, SharedObservationEvent};
use parking_lot::Mutex;
use serde::Serialize;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

const OBSERVATION_THREAD_STACK_BYTES: usize = 512 * 1024;

/// Stable marker written as the first line of every observation JSONL file.
pub const OBSERVATION_FILE_FORMAT_V1: &str = "# appcore-observations-v1";
/// Maximum queued observation bytes, including the event currently being written.
pub const MAX_FILE_OBSERVATION_QUEUE_BYTES: u64 = 8 * 1024 * 1024;
/// Maximum item capacity accepted by the file observation queue.
pub const MAX_FILE_OBSERVATION_QUEUE_ITEMS: usize = 65_536;
/// Default deadline shared by flush queue admission and acknowledgement.
pub const FILE_OBSERVATION_FLUSH_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_FILE_OBSERVATION_RECORD_BYTES: u64 = 256 * 1024;

/// Local observation drain limits and retention policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileObservationSinkConfig {
    /// Active JSONL file path.
    pub path: PathBuf,
    /// Maximum active file size before rotation.
    pub max_file_bytes: u64,
    /// Number of rotated files retained.
    pub retained_files: usize,
    /// Maximum queued observations awaiting disk.
    pub queue_capacity: usize,
    /// Number of records written between `fsync` calls.
    pub sync_every_records: usize,
}

impl FileObservationSinkConfig {
    /// Creates a production-safe bounded configuration.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            max_file_bytes: 16 * 1024 * 1024,
            retained_files: 4,
            queue_capacity: 4_096,
            sync_every_records: 64,
        }
    }

    fn validate(&self) -> std::io::Result<()> {
        if self.max_file_bytes < 64 * 1024
            || self.retained_files == 0
            || self.queue_capacity == 0
            || self.queue_capacity > MAX_FILE_OBSERVATION_QUEUE_ITEMS
            || self.sync_every_records == 0
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "observation drain limits must be positive, max_file_bytes >= 64 KiB and queue_capacity <= 65536",
            ));
        }
        Ok(())
    }
}

/// Point-in-time counters for one file observation drain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileObservationSinkStats {
    /// Records durably accepted by the worker.
    pub written: u64,
    /// Records discarded because the bounded queue was full.
    pub dropped: u64,
    /// Filesystem or serialization failures.
    pub errors: u64,
}

/// Aggregate memory pressure for the file observation queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileObservationSinkPressure {
    /// Bytes retained by queued and currently written observations.
    pub queued_bytes: u64,
    /// Hard aggregate byte ceiling.
    pub max_queue_bytes: u64,
    /// Highest retained byte count observed since startup.
    pub peak_queued_bytes: u64,
    /// Events rejected because the aggregate byte ceiling was full.
    pub byte_rejections: u64,
}

enum DrainCommand {
    Event(QueuedEvent),
    Flush(mpsc::Sender<()>),
}

struct QueuedEvent {
    event: SharedObservationEvent,
    line_bytes: u64,
    _reservation: QueueReservation,
}

struct QueueReservation {
    budget: Arc<QueueBudget>,
    bytes: u64,
}

impl Drop for QueueReservation {
    fn drop(&mut self) {
        self.budget.used.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

struct QueueBudget {
    max_bytes: u64,
    used: AtomicU64,
    peak: AtomicU64,
    rejected: AtomicU64,
}

impl QueueBudget {
    const fn new(max_bytes: u64) -> Self {
        Self {
            max_bytes,
            used: AtomicU64::new(0),
            peak: AtomicU64::new(0),
            rejected: AtomicU64::new(0),
        }
    }

    fn reserve(self: &Arc<Self>, bytes: u64) -> Option<QueueReservation> {
        let mut used = self.used.load(Ordering::Acquire);
        loop {
            let Some(next) = used.checked_add(bytes) else {
                self.rejected.fetch_add(1, Ordering::Relaxed);
                return None;
            };
            if next > self.max_bytes {
                self.rejected.fetch_add(1, Ordering::Relaxed);
                return None;
            }
            match self
                .used
                .compare_exchange_weak(used, next, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => {
                    self.peak.fetch_max(next, Ordering::Relaxed);
                    return Some(QueueReservation {
                        budget: Arc::clone(self),
                        bytes,
                    });
                }
                Err(current) => used = current,
            }
        }
    }

    fn pressure(&self) -> FileObservationSinkPressure {
        FileObservationSinkPressure {
            queued_bytes: self.used.load(Ordering::Acquire),
            max_queue_bytes: self.max_bytes,
            peak_queued_bytes: self.peak.load(Ordering::Relaxed),
            byte_rejections: self.rejected.load(Ordering::Relaxed),
        }
    }
}

struct FileObservationSinkInner {
    sender: Mutex<Option<SyncSender<DrainCommand>>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    written: Arc<AtomicU64>,
    dropped: AtomicU64,
    errors: Arc<AtomicU64>,
    queue_budget: Arc<QueueBudget>,
    max_file_bytes: u64,
}

impl Drop for FileObservationSinkInner {
    fn drop(&mut self) {
        self.sender.get_mut().take();
        if let Some(worker) = self.worker.get_mut().take() {
            let _ = worker.join();
        }
    }
}

/// Cloneable non-blocking observation sink backed by a bounded worker queue.
#[derive(Clone)]
pub struct FileObservationSink {
    inner: Arc<FileObservationSinkInner>,
}

impl std::fmt::Debug for FileObservationSink {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FileObservationSink")
            .field("stats", &self.stats())
            .field("pressure", &self.pressure())
            .finish()
    }
}

impl FileObservationSink {
    /// Creates the active file and starts the bounded writer worker.
    pub fn new(config: FileObservationSinkConfig) -> std::io::Result<Self> {
        config.validate()?;
        initialize_file(&config.path)?;
        let (sender, receiver) = mpsc::sync_channel(config.queue_capacity);
        let written = Arc::new(AtomicU64::new(0));
        let errors = Arc::new(AtomicU64::new(0));
        let worker_written = Arc::clone(&written);
        let worker_errors = Arc::clone(&errors);
        let queue_budget = Arc::new(QueueBudget::new(MAX_FILE_OBSERVATION_QUEUE_BYTES));
        let max_file_bytes = config.max_file_bytes;
        let worker = std::thread::Builder::new()
            .name("appcore-observation-drain".to_string())
            .stack_size(OBSERVATION_THREAD_STACK_BYTES)
            .spawn(move || run_worker(config, receiver, worker_written, worker_errors))?;
        Ok(Self {
            inner: Arc::new(FileObservationSinkInner {
                sender: Mutex::new(Some(sender)),
                worker: Mutex::new(Some(worker)),
                written,
                dropped: AtomicU64::new(0),
                errors,
                queue_budget,
                max_file_bytes,
            }),
        })
    }

    /// Flushes all events accepted before this call.
    pub fn flush(&self) -> std::io::Result<()> {
        self.flush_timeout(FILE_OBSERVATION_FLUSH_TIMEOUT)
    }

    /// Flushes accepted events within one deadline covering queue admission
    /// and worker acknowledgement.
    pub fn flush_timeout(&self, timeout: Duration) -> std::io::Result<()> {
        let deadline = crate::observation_flush::deadline(timeout)?;
        let (acknowledge, receiver) = mpsc::channel();
        let sender = self.inner.sender.lock().clone().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::BrokenPipe, "observation drain stopped")
        })?;
        crate::observation_flush::enqueue(&sender, DrainCommand::Flush(acknowledge), deadline)?;
        crate::observation_flush::wait(receiver, deadline)
    }

    /// Returns worker, backpressure and failure counters.
    pub fn stats(&self) -> FileObservationSinkStats {
        FileObservationSinkStats {
            written: self.inner.written.load(Ordering::Relaxed),
            dropped: self.inner.dropped.load(Ordering::Relaxed),
            errors: self.inner.errors.load(Ordering::Relaxed),
        }
    }

    /// Returns aggregate byte pressure for queued and currently written events.
    pub fn pressure(&self) -> FileObservationSinkPressure {
        self.inner.queue_budget.pressure()
    }

    fn enqueue_shared(&self, event: SharedObservationEvent) {
        let line_bytes = match measure_event_line(self.inner.max_file_bytes, event.as_event()) {
            Ok(bytes) => bytes,
            Err(_) => {
                self.inner.errors.fetch_add(1, Ordering::Relaxed);
                return;
            }
        };
        let Some(reservation) = self.inner.queue_budget.reserve(line_bytes) else {
            self.inner.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        };
        let event = QueuedEvent {
            event,
            line_bytes,
            _reservation: reservation,
        };
        let Some(sender) = self.inner.sender.lock().clone() else {
            self.inner.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        };
        match sender.try_send(DrainCommand::Event(event)) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
                self.inner.dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

impl ObservationSink for FileObservationSink {
    fn emit(&self, event: ObservationEvent) {
        self.enqueue_shared(SharedObservationEvent::new(event));
    }

    fn emit_shared(&self, event: &SharedObservationEvent) {
        self.enqueue_shared(event.clone());
    }
}

fn run_worker(
    config: FileObservationSinkConfig,
    receiver: Receiver<DrainCommand>,
    written: Arc<AtomicU64>,
    errors: Arc<AtomicU64>,
) {
    let mut unsynced = 0usize;
    let mut file = open_append(&config.path).ok();
    while let Ok(command) = receiver.recv() {
        match command {
            DrainCommand::Event(event) => {
                let result =
                    write_event(&config, &mut file, event.event.as_event(), event.line_bytes);
                if result.is_ok() {
                    written.fetch_add(1, Ordering::Relaxed);
                    unsynced += 1;
                } else {
                    errors.fetch_add(1, Ordering::Relaxed);
                }
                if unsynced >= config.sync_every_records {
                    sync_file(&mut file, &errors);
                    unsynced = 0;
                }
            }
            DrainCommand::Flush(acknowledge) => {
                sync_file(&mut file, &errors);
                unsynced = 0;
                let _ = acknowledge.send(());
            }
        }
    }
    sync_file(&mut file, &errors);
}

fn write_event(
    config: &FileObservationSinkConfig,
    file: &mut Option<File>,
    event: &ObservationEvent,
    line_bytes: u64,
) -> std::io::Result<()> {
    let current_size = file
        .as_ref()
        .and_then(|file| file.metadata().ok())
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    if current_size
        .checked_add(line_bytes)
        .is_none_or(|bytes| bytes > config.max_file_bytes)
    {
        if let Some(active) = file.take() {
            active.sync_all()?;
        }
        rotate_files(config)?;
        *file = Some(open_append(&config.path)?);
    }
    if file.is_none() {
        *file = Some(open_append(&config.path)?);
    }
    match file.as_mut() {
        Some(file) => write_event_line(file, event),
        None => Err(std::io::Error::other(
            "observation file was not initialized",
        )),
    }
}

fn measure_event_line(max_file_bytes: u64, event: &ObservationEvent) -> std::io::Result<u64> {
    let header_bytes = u64::try_from(OBSERVATION_FILE_FORMAT_V1.len())
        .ok()
        .and_then(|bytes| bytes.checked_add(1))
        .ok_or_else(|| std::io::Error::other("observation size overflow"))?;
    let file_json_limit = max_file_bytes
        .checked_sub(header_bytes)
        .and_then(|bytes| bytes.checked_sub(1))
        .ok_or_else(record_too_large)?;
    let record_json_limit = MAX_FILE_OBSERVATION_RECORD_BYTES
        .checked_sub(1)
        .ok_or_else(record_too_large)?;
    let json_limit = file_json_limit.min(record_json_limit);
    let mut counter = LimitedCounter::new(json_limit);
    let result = serde_json::to_writer(&mut counter, &VersionedObservation::new(event));
    if counter.exceeded {
        return Err(record_too_large());
    }
    result.map_err(std::io::Error::other)?;
    counter
        .bytes
        .checked_add(1)
        .ok_or_else(|| std::io::Error::other("observation size overflow"))
}

fn write_event_line(writer: &mut impl Write, event: &ObservationEvent) -> std::io::Result<()> {
    serde_json::to_writer(&mut *writer, &VersionedObservation::new(event))
        .map_err(std::io::Error::other)?;
    writer.write_all(b"\n")
}

fn record_too_large() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "observation record exceeds file limit",
    )
}

#[derive(Serialize)]
struct VersionedObservation<'a> {
    schema: &'static str,
    event: &'a ObservationEvent,
}

impl<'a> VersionedObservation<'a> {
    fn new(event: &'a ObservationEvent) -> Self {
        Self {
            schema: "appcore.observation.v1",
            event,
        }
    }
}

struct LimitedCounter {
    bytes: u64,
    limit: u64,
    exceeded: bool,
}

impl LimitedCounter {
    const fn new(limit: u64) -> Self {
        Self {
            bytes: 0,
            limit,
            exceeded: false,
        }
    }
}

impl Write for LimitedCounter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let Some(bytes) = self.bytes.checked_add(buffer.len() as u64) else {
            self.exceeded = true;
            return Err(std::io::Error::other("observation size overflow"));
        };
        if bytes > self.limit {
            self.exceeded = true;
            return Err(record_too_large());
        }
        self.bytes = bytes;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn initialize_file(path: &Path) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    reject_symlink(path)?;
    if !path.exists() {
        let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
        writeln!(file, "{OBSERVATION_FILE_FORMAT_V1}")?;
        file.sync_all()?;
        sync_parent(parent)?;
        return Ok(());
    }
    let mut first = String::new();
    BufReader::new(File::open(path)?).read_line(&mut first)?;
    if first.trim_end() != OBSERVATION_FILE_FORMAT_V1 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "unsupported observation file format",
        ));
    }
    Ok(())
}

fn open_append(path: &Path) -> std::io::Result<File> {
    initialize_file(path)?;
    OpenOptions::new().append(true).read(true).open(path)
}

fn rotate_files(config: &FileObservationSinkConfig) -> std::io::Result<()> {
    for index in (1..=config.retained_files).rev() {
        let source = rotated_path(&config.path, index);
        if index == config.retained_files {
            remove_if_exists(&source)?;
        } else if source.exists() {
            fs::rename(&source, rotated_path(&config.path, index + 1))?;
        }
    }
    if config.path.exists() {
        fs::rename(&config.path, rotated_path(&config.path, 1))?;
    }
    initialize_file(&config.path)
}

fn rotated_path(path: &Path, index: usize) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("observations.jsonl");
    path.with_file_name(format!("{name}.{index}"))
}

fn reject_symlink(path: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "observation path must not be a symlink",
        )),
        Ok(metadata) if !metadata.is_file() => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "observation path must be a regular file",
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn sync_file(file: &mut Option<File>, errors: &AtomicU64) {
    if file.as_ref().is_some_and(|file| file.sync_all().is_err()) {
        errors.fetch_add(1, Ordering::Relaxed);
    }
}

fn remove_if_exists(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> std::io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
#[path = "observation_file_tests.rs"]
mod tests;
