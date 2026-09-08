// =============================================================================
//        #######
//     ###       ###     F: sink.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: unknown by dnettoRaw
//    ##   ## ##   ##    U: working-tree by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Concrete bounded sinks. Sink failures return to the dispatcher without recursion.

use crate::LogEvent;
use appcore_contracts::ApplicationId;
use appcore_dnt::{
    write_atomic, BytesCodec, ContentType, DntKeyProvider, DntOpenOptions, DntSealOptions, KeyId,
};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

/// Logging sink error with no sensitive source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogError {
    /// Output failed or was unavailable.
    Io,
    /// A bounded sink rejected the event.
    Capacity,
    /// DNT sealing or validation failed.
    Encryption,
    /// Event serialization failed.
    Serialization,
}

/// A destination receiving a sanitized event from [`crate::LogDispatcher`].
pub trait LogSink: Send + Sync {
    /// Writes one event or reports a controlled failure.
    fn emit(&self, event: &LogEvent) -> Result<(), LogError>;

    /// Reports whether this sink is an explicitly encrypted sensitive boundary.
    ///
    /// Ordinary sinks deliberately retain the default `false`, so a sensitive
    /// policy can never disclose an event to stdout, JSONL or memory by mistake.
    fn accepts_sensitive(&self) -> bool {
        false
    }

    /// Stable bounded metric label for this sink implementation.
    fn name(&self) -> &'static str {
        "custom"
    }
}

/// Human-oriented stdout sink, with no terminal-color assumptions.
#[derive(Debug, Default)]
pub struct ConsoleSink;

impl ConsoleSink {
    /// Creates a plain console sink.
    pub fn new() -> Self {
        Self
    }
}

impl LogSink for ConsoleSink {
    fn emit(&self, event: &LogEvent) -> Result<(), LogError> {
        writeln!(
            std::io::stdout().lock(),
            "[{:?}][{}][V{}] {}",
            event.severity,
            event.component,
            event.verbosity.value(),
            event.message
        )
        .map_err(|_| LogError::Io)
    }

    fn name(&self) -> &'static str {
        "console"
    }
}

/// Bounded structured JSONL file configuration.
#[derive(Debug, Clone)]
pub struct FileSinkConfig {
    /// Target JSONL file.
    pub path: PathBuf,
    /// Maximum file bytes before rotation.
    pub max_bytes: u64,
    /// Flush each event to storage; disable to favor throughput and OS buffering.
    pub sync_each_write: bool,
    /// Number of old files retained.
    pub retention: u8,
    /// Optional bounded year/month archive for files leaving active retention.
    pub archive: Option<FileArchiveConfig>,
}

/// Bounded archive configuration for rotated JSONL files.
#[derive(Debug, Clone)]
pub struct FileArchiveConfig {
    /// Archive root containing `YYYY/MM` subdirectories.
    pub directory: PathBuf,
    /// Maximum archived files across the complete archive root.
    pub max_files: usize,
}

/// Synchronous bounded JSONL sink; callers choose it only outside hot paths.
pub struct FileSink {
    config: FileSinkConfig,
    state: Mutex<FileSinkState>,
}

struct FileSinkState {
    file: Option<File>,
    bytes: u64,
}

impl FileSink {
    /// Creates a sink after validating nonzero size and bounded retention.
    pub fn new(config: FileSinkConfig) -> Result<Self, LogError> {
        if config.max_bytes == 0
            || config.retention > 32
            || config
                .archive
                .as_ref()
                .is_some_and(|archive| archive.max_files == 0 || archive.max_files > 10_000)
        {
            return Err(LogError::Capacity);
        }
        Ok(Self {
            config,
            state: Mutex::new(FileSinkState {
                file: None,
                bytes: 0,
            }),
        })
    }

    fn rotate(&self, incoming_bytes: usize, timestamp_ms: u64) -> Result<(), LogError> {
        let path = &self.config.path;
        ensure_regular_or_missing(path)?;
        let incoming_bytes = u64::try_from(incoming_bytes).map_err(|_| LogError::Capacity)?;
        let requires_rotation = fs::metadata(path)
            .map(|metadata| metadata.len().saturating_add(incoming_bytes) > self.config.max_bytes)
            .unwrap_or(false);
        if !requires_rotation {
            return Ok(());
        }
        if self.config.retention == 0 {
            if let Some(archive) = &self.config.archive {
                archive_file(path, path, timestamp_ms, archive)?;
            } else {
                fs::remove_file(path).map_err(|_| LogError::Io)?;
            }
            return Ok(());
        }
        let oldest = path.with_extension(format!("jsonl.{}", self.config.retention));
        ensure_regular_or_missing(&oldest)?;
        if oldest.exists() {
            if let Some(archive) = &self.config.archive {
                archive_file(&oldest, path, timestamp_ms, archive)?;
            } else {
                fs::remove_file(oldest).map_err(|_| LogError::Io)?;
            }
        }
        for index in (1..self.config.retention).rev() {
            let from = path.with_extension(format!("jsonl.{index}"));
            let to = path.with_extension(format!("jsonl.{}", index + 1));
            ensure_regular_or_missing(&from)?;
            ensure_regular_or_missing(&to)?;
            if from.exists() {
                fs::rename(from, to).map_err(|_| LogError::Io)?;
            }
        }
        if path.exists() {
            fs::rename(path, path.with_extension("jsonl.1")).map_err(|_| LogError::Io)?;
        }
        Ok(())
    }
}

impl LogSink for FileSink {
    fn emit(&self, event: &LogEvent) -> Result<(), LogError> {
        let mut state = self.state.lock();
        let line = crate::json_line::encode(event, self.config.max_bytes)?;
        ensure_regular_or_missing(&self.config.path)?;
        reconcile_file_state(&self.config.path, &mut state)?;
        let incoming = u64::try_from(line.len()).map_err(|_| LogError::Capacity)?;
        let mut next_bytes = state
            .bytes
            .checked_add(incoming)
            .ok_or(LogError::Capacity)?;
        if next_bytes > self.config.max_bytes {
            state.file.take();
            self.rotate(line.len(), event.timestamp_ms)?;
            state.bytes = 0;
            next_bytes = incoming;
        }
        if state.file.is_none() {
            state.file = Some(open_append_regular(&self.config.path)?);
            state.bytes = state
                .file
                .as_ref()
                .ok_or(LogError::Io)?
                .metadata()
                .map_err(|_| LogError::Io)?
                .len();
        }
        let Some(file) = state.file.as_mut() else {
            return Err(LogError::Io);
        };
        if file.write_all(&line).is_err() {
            state.file = None;
            return Err(LogError::Io);
        }
        state.bytes = next_bytes;
        if self.config.sync_each_write
            && state
                .file
                .as_ref()
                .ok_or(LogError::Io)?
                .sync_data()
                .is_err()
        {
            state.file = None;
            return Err(LogError::Io);
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "file"
    }
}

fn reconcile_file_state(path: &std::path::Path, state: &mut FileSinkState) -> Result<(), LogError> {
    let Some(file) = state.file.as_ref() else {
        state.bytes = fs::metadata(path).map_or(0, |metadata| metadata.len());
        return Ok(());
    };
    let Ok(path_metadata) = fs::metadata(path) else {
        state.file = None;
        state.bytes = 0;
        return Ok(());
    };
    let file_metadata = file.metadata().map_err(|_| LogError::Io)?;
    if !same_file(&file_metadata, &path_metadata) || path_metadata.len() != state.bytes {
        state.file = None;
        state.bytes = path_metadata.len();
    }
    Ok(())
}

#[cfg(unix)]
fn same_file(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(not(unix))]
fn same_file(_left: &fs::Metadata, _right: &fs::Metadata) -> bool {
    true
}

fn open_append_regular(path: &std::path::Path) -> Result<File, LogError> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    let file = options.open(path).map_err(|_| LogError::Io)?;
    let path_metadata = fs::symlink_metadata(path).map_err(|_| LogError::Io)?;
    let file_metadata = file.metadata().map_err(|_| LogError::Io)?;
    if path_metadata.file_type().is_symlink()
        || !file_metadata.is_file()
        || !same_file(&file_metadata, &path_metadata)
    {
        return Err(LogError::Io);
    }
    Ok(file)
}

fn archive_file(
    source: &std::path::Path,
    active_path: &std::path::Path,
    timestamp_ms: u64,
    config: &FileArchiveConfig,
) -> Result<(), LogError> {
    let (year, month) = year_month(timestamp_ms);
    let year_dir = config.directory.join(format!("{year:04}"));
    let destination_dir = year_dir.join(format!("{month:02}"));
    ensure_directory_or_missing(&config.directory)?;
    ensure_directory_or_missing(&year_dir)?;
    ensure_directory_or_missing(&destination_dir)?;
    fs::create_dir_all(&destination_dir).map_err(|_| LogError::Io)?;
    ensure_directory(&config.directory)?;
    ensure_directory(&year_dir)?;
    ensure_directory(&destination_dir)?;
    prune_archive(&config.directory, config.max_files.saturating_sub(1))?;

    let stem = active_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("log");
    let extension = active_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("jsonl");
    for sequence in 0_u16..=u16::MAX {
        let destination = destination_dir.join(format!(
            "{stem}-{timestamp_ms:020}-{sequence:04}.{extension}"
        ));
        ensure_regular_or_missing(&destination)?;
        if !destination.exists() {
            return fs::rename(source, destination).map_err(|_| LogError::Io);
        }
    }
    Err(LogError::Capacity)
}

fn prune_archive(root: &std::path::Path, keep: usize) -> Result<(), LogError> {
    let mut files = Vec::new();
    if root.exists() {
        ensure_directory(root)?;
        for year in fs::read_dir(root).map_err(|_| LogError::Io)? {
            let year = year.map_err(|_| LogError::Io)?.path();
            let year_metadata = fs::symlink_metadata(&year).map_err(|_| LogError::Io)?;
            if !year_metadata.file_type().is_dir() {
                continue;
            }
            for month in fs::read_dir(year).map_err(|_| LogError::Io)? {
                let month = month.map_err(|_| LogError::Io)?.path();
                let month_metadata = fs::symlink_metadata(&month).map_err(|_| LogError::Io)?;
                if !month_metadata.file_type().is_dir() {
                    continue;
                }
                for file in fs::read_dir(month).map_err(|_| LogError::Io)? {
                    let file = file.map_err(|_| LogError::Io)?.path();
                    let metadata = fs::symlink_metadata(&file).map_err(|_| LogError::Io)?;
                    if metadata.file_type().is_file() {
                        files.push(file);
                    }
                }
            }
        }
    }
    files.sort();
    let remove = files.len().saturating_sub(keep);
    for file in files.into_iter().take(remove) {
        fs::remove_file(file).map_err(|_| LogError::Io)?;
    }
    Ok(())
}

fn year_month(timestamp_ms: u64) -> (i64, i64) {
    let days = i64::try_from(timestamp_ms / 86_400_000).unwrap_or(i64::MAX);
    let shifted = days.saturating_add(719_468);
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    (year, month)
}

/// In-memory bounded diagnostic sink.
pub struct RingBufferSink {
    state: Mutex<(VecDeque<(LogEvent, usize)>, usize)>,
    max_events: usize,
    max_bytes: usize,
}

impl RingBufferSink {
    /// Creates a ring bounded by events and estimated event bytes.
    pub fn new(max_events: usize, max_bytes: usize) -> Result<Self, LogError> {
        if max_events == 0 || max_bytes == 0 {
            return Err(LogError::Capacity);
        }
        Ok(Self {
            state: Mutex::new((VecDeque::new(), 0)),
            max_events,
            max_bytes,
        })
    }

    /// Returns a sanitized diagnostic snapshot.
    pub fn snapshot(&self) -> Vec<LogEvent> {
        self.state
            .lock()
            .0
            .iter()
            .map(|(event, _)| event.clone())
            .collect()
    }
}

impl LogSink for RingBufferSink {
    fn emit(&self, event: &LogEvent) -> Result<(), LogError> {
        let size = event.retained_bytes();
        if size > self.max_bytes {
            return Err(LogError::Capacity);
        }
        let mut state = self.state.lock();
        while state.0.len() >= self.max_events || state.1.saturating_add(size) > self.max_bytes {
            let Some((_, old_size)) = state.0.pop_front() else {
                break;
            };
            state.1 = state.1.saturating_sub(old_size);
        }
        state.1 = state.1.saturating_add(size);
        state.0.push_back((event.clone(), size));
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ring"
    }
}

/// Explicit encrypted sensitive-output configuration.
#[derive(Debug, Clone)]
pub struct SensitiveDntSinkConfig {
    /// DNT output file.
    pub path: PathBuf,
    /// Owning application identity.
    pub application_id: ApplicationId,
    /// Rotation-aware DNT key identity.
    pub key_id: KeyId,
    /// Maximum plaintext snapshot bytes.
    pub max_bytes: u64,
    /// Maximum events retained in the encrypted snapshot.
    pub max_events: usize,
    /// Number of prior encrypted snapshots retained beside the current file.
    pub retention: u8,
}

/// Encrypted DNT sink. It has no plaintext fallback and must be explicitly wired.
pub struct SensitiveDntSink<P: DntKeyProvider> {
    config: SensitiveDntSinkConfig,
    key_provider: P,
    state: Mutex<(VecDeque<(LogEvent, usize)>, usize)>,
}

impl<P: DntKeyProvider> SensitiveDntSink<P> {
    /// Creates the sink; enabling it is an explicit sensitive-logging decision.
    pub fn new(config: SensitiveDntSinkConfig, key_provider: P) -> Result<Self, LogError> {
        if config.max_bytes == 0 || config.max_events == 0 || config.retention > 32 {
            return Err(LogError::Capacity);
        }
        Ok(Self {
            config,
            key_provider,
            state: Mutex::new((VecDeque::new(), 0)),
        })
    }
}

impl<P: DntKeyProvider> LogSink for SensitiveDntSink<P> {
    fn emit(&self, event: &LogEvent) -> Result<(), LogError> {
        let event_size = event.retained_bytes();
        if u64::try_from(event_size).map_err(|_| LogError::Capacity)? > self.config.max_bytes {
            return Err(LogError::Capacity);
        }
        let mut state = self.state.lock();
        let max_bytes = usize::try_from(self.config.max_bytes).map_err(|_| LogError::Capacity)?;
        while state.0.len() >= self.config.max_events
            || state.1.saturating_add(event_size) > max_bytes
        {
            let Some((_, old_size)) = state.0.pop_front() else {
                break;
            };
            state.1 = state.1.saturating_sub(old_size);
        }
        state.1 = state.1.saturating_add(event_size);
        state.0.push_back((event.clone(), event_size));
        let events = state.0.iter().map(|(stored, _)| stored).collect::<Vec<_>>();
        let payload = serde_json::to_vec(&events).map_err(|_| LogError::Serialization)?;
        if u64::try_from(payload.len()).map_err(|_| LogError::Capacity)? > self.config.max_bytes {
            let _ = state.0.pop_back();
            state.1 = state.1.saturating_sub(event_size);
            return Err(LogError::Capacity);
        }
        let content =
            ContentType::new("appcore.log.sensitive").map_err(|_| LogError::Encryption)?;
        let seal = DntSealOptions {
            application_id: self.config.application_id.clone(),
            tenant_id: None,
            content_type: content.clone(),
            schema_version: 1,
            key_id: self.config.key_id.clone(),
            created_at_ms: event.timestamp_ms,
            public_metadata:
                b"sensitivity=sensitive;contains_secrets=true;encrypted=true;format=dnt".to_vec(),
            encrypted_metadata: Vec::new(),
            flags: 0,
            max_payload_bytes: Some(self.config.max_bytes),
        };
        let open = DntOpenOptions {
            application_id: self.config.application_id.clone(),
            tenant_id: None,
            content_type: content,
            max_payload_bytes: Some(self.config.max_bytes),
        };
        rotate_sensitive_snapshots(&self.config.path, self.config.retention)?;
        write_atomic(
            &self.config.path,
            &payload,
            &self.key_provider,
            &BytesCodec,
            seal,
            &open,
        )
        .map_err(|_| LogError::Encryption)
    }

    fn accepts_sensitive(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "sensitive_dnt"
    }
}

fn rotate_sensitive_snapshots(path: &std::path::Path, retention: u8) -> Result<(), LogError> {
    ensure_regular_or_missing(path)?;
    if !path.exists() {
        return Ok(());
    }
    if retention == 0 {
        fs::remove_file(path).map_err(|_| LogError::Io)?;
        return Ok(());
    }
    let oldest = path.with_extension(format!("dnt.{retention}"));
    ensure_regular_or_missing(&oldest)?;
    if oldest.exists() {
        fs::remove_file(&oldest).map_err(|_| LogError::Io)?;
    }
    for index in (1..retention).rev() {
        let from = path.with_extension(format!("dnt.{index}"));
        let to = path.with_extension(format!("dnt.{}", index + 1));
        ensure_regular_or_missing(&from)?;
        ensure_regular_or_missing(&to)?;
        if from.exists() {
            fs::rename(from, to).map_err(|_| LogError::Io)?;
        }
    }
    fs::rename(path, path.with_extension("dnt.1")).map_err(|_| LogError::Io)
}

fn ensure_regular_or_missing(path: &std::path::Path) -> Result<(), LogError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.file_type().is_file() => {
            Err(LogError::Io)
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(LogError::Io),
    }
}

fn ensure_directory_or_missing(path: &std::path::Path) -> Result<(), LogError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => Ok(()),
        Ok(_) => Err(LogError::Io),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(LogError::Io),
    }
}

fn ensure_directory(path: &std::path::Path) -> Result<(), LogError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| LogError::Io)?;
    if metadata.file_type().is_dir() {
        Ok(())
    } else {
        Err(LogError::Io)
    }
}
