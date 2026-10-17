// =============================================================================
//        #######
//     ###       ###     F: file.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 10:22:10 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 2.0.0-alpha.6
// =============================================================================

use super::format::{digest, History};
use super::{
    invalid, validate_identifier, CanonicalHeadV1, CanonicalStreamV1, MAX_CANONICAL_HISTORY_BYTES,
};
use super::{CanonicalBatchV1 as Batch, DurabilityReceiptV1, DurabilityTargetV1};
use crate::sync::persistence::{atomic_write, reject_symlink};
use crate::sync::{SyncMessage, SyncResult};
use appcore_distributed_contracts::ServiceLeaderLease;
use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};

/// Bounded durable reference history. Never implicitly initializes a missing file.
/// Paths must be on trusted, owner-controlled storage; I/O belongs off async threads.
/// No compaction, automatic restore, election or application materialization is provided.
#[derive(Clone)]
pub struct FileCanonicalHistoryV1 {
    path: PathBuf,
    stream: CanonicalStreamV1,
}

/// Locked head verification and append boundary. Never retain or reenter this transaction.
/// The host must hold its authoritative fencing transaction around writer operations.
pub struct CanonicalHistoryTransactionV1<'a> {
    path: &'a Path,
    history: History,
    failed: bool,
}

impl FileCanonicalHistoryV1 {
    /// Explicitly creates an empty history in an existing trusted directory.
    /// Existing files are rejected, including corrupt ones. Never use creation to recover loss.
    pub fn create(root: &Path, name: &str, stream: CanonicalStreamV1) -> SyncResult<Self> {
        let store = Self::location(root, name, stream)?;
        let _lock = store.lock()?;
        if store.path.try_exists().map_err(io_error)? {
            return Err(invalid("canonical history already exists"));
        }
        let history = History {
            format_version: 1,
            stream: store.stream.clone(),
            batches: Vec::new(),
        };
        save(&store.path, &history)?;
        Ok(store)
    }

    /// Opens only an existing, fully validated history; loss/corruption fails closed.
    pub fn open_existing(root: &Path, name: &str, stream: CanonicalStreamV1) -> SyncResult<Self> {
        let store = Self::location(root, name, stream)?;
        let _lock = store.lock()?;
        store.load()?;
        Ok(store)
    }

    /// Revalidates durable bytes and returns the exact logical head, not a cached maximum.
    pub fn head(&self) -> SyncResult<CanonicalHeadV1> {
        let _lock = self.lock()?;
        Ok(self.load()?.head())
    }

    /// Recovers an immutable durability target at a retained batch boundary.
    /// A later local head is allowed only when the requested head is an exact ancestor.
    pub fn durability_target(&self, required: &CanonicalHeadV1) -> SyncResult<DurabilityTargetV1> {
        let _lock = self.lock()?;
        target_at_head(&self.load()?, required)
    }

    /// Reconstructs a receipt from validated retained bytes, syncing file and directory again.
    /// Provision `local_core_id` from the host identity, never from an incoming request.
    /// Authenticate the outgoing receipt as that Core. This is a trusted provider boundary,
    /// not proof of application materialization, device health or current write authority.
    /// Lost replies can be retried after restart even when this history has advanced.
    pub fn durability_receipt(
        &self,
        target: &DurabilityTargetV1,
        local_core_id: &str,
    ) -> SyncResult<DurabilityReceiptV1> {
        target.validate()?;
        validate_identifier(local_core_id)?;
        let _lock = self.lock()?;
        let observed = target_at_head(&self.load()?, &target.head)?;
        if &observed != target {
            return Err(invalid("durability target metadata mismatch"));
        }
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.path)
            .map_err(io_error)?
            .sync_all()
            .map_err(io_error)?;
        crate::sync::persistence::sync_parent(self.path.parent().unwrap())?;
        Ok(DurabilityReceiptV1 {
            schema_version: 1,
            target: observed,
            persisted_by_core_id: local_core_id.into(),
        })
    }

    /// Copies one original batch, including its historical writer and epoch.
    /// Authenticate the source and grant before calling; historical grants need not be live.
    /// An identical persisted replay returns the current head, even after restart.
    /// Divergence, gaps, backwards epochs and capacity exhaustion leave bytes unchanged.
    pub fn append_authenticated(
        &self,
        lease: &ServiceLeaderLease,
        message: SyncMessage,
    ) -> SyncResult<CanonicalHeadV1> {
        let _lock = self.lock()?;
        let mut transaction = CanonicalHistoryTransactionV1 {
            path: &self.path,
            history: self.load()?,
            failed: false,
        };
        transaction.append_authenticated(lease, message)
    }

    /// Persists one authenticated replica record with its exact lineage digest.
    /// Does not grant local write leadership or validate historical grant authenticity.
    pub fn replicate_authenticated(&self, batch: Batch) -> SyncResult<CanonicalHeadV1> {
        let _lock = self.lock()?;
        let mut transaction = CanonicalHistoryTransactionV1 {
            path: &self.path,
            history: self.load()?,
            failed: false,
        };
        transaction.append_batch(batch)
    }

    /// Returns at most `limit` original batches after a batch offset (maximum 64).
    /// The entire reference history is read within its fixed storage budget first.
    /// Retains epoch/writer evidence. Transport must authenticate the complete record.
    pub fn batches(&self, offset: usize, limit: usize) -> SyncResult<Vec<Batch>> {
        if limit == 0 || limit > 64 {
            return Err(invalid("invalid canonical page limit"));
        }
        let _lock = self.lock()?;
        let history = self.load()?;
        if offset > history.batches.len() {
            return Err(invalid("invalid canonical batch offset"));
        }
        Ok(history
            .batches
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect())
    }

    /// Executes a local action only when durable history equals an explicit required head.
    /// Obtain that head from a trusted durability policy, never from a candidate's own report.
    /// This callback is NOT a lease check: compose an authoritative fence around the commit
    /// inside it, e.g. FileControlPlane::with_fenced_service_write for co-located storage.
    /// Use a consistent history-then-authority lock order. Do not reenter either provider.
    /// A remote lease observation cannot authorize a later unfenced commit.
    /// Each successful append commits immediately; callback errors do not roll it back.
    pub fn with_verified_head<T>(
        &self,
        required: &CanonicalHeadV1,
        action: impl FnOnce(&mut CanonicalHistoryTransactionV1<'_>) -> SyncResult<T>,
    ) -> SyncResult<T> {
        let _lock = self.lock()?;
        let history = self.load()?;
        if &history.head() != required {
            return Err(invalid("canonical promotion head mismatch"));
        }
        action(&mut CanonicalHistoryTransactionV1 {
            path: &self.path,
            history,
            failed: false,
        })
    }

    fn location(root: &Path, name: &str, stream: CanonicalStreamV1) -> SyncResult<Self> {
        stream.validate()?;
        validate_identifier(name)?;
        if name == "." || name == ".." {
            return Err(invalid("invalid canonical file name"));
        }
        let root = root.canonicalize().map_err(io_error)?;
        if !root.is_dir() {
            return Err(invalid("canonical storage root is not a directory"));
        }
        Ok(Self {
            path: root.join(name),
            stream,
        })
    }

    fn lock(&self) -> SyncResult<File> {
        let lock = self.path.with_file_name(format!(
            ".{}.lock",
            self.path.file_name().unwrap().to_string_lossy()
        ));
        reject_symlink(&lock)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock)
            .map_err(io_error)?;
        file.try_lock_exclusive()
            .map_err(|_| invalid("canonical history busy or unavailable"))?;
        Ok(file)
    }

    fn load(&self) -> SyncResult<History> {
        reject_symlink(&self.path)?;
        let mut bytes = Vec::new();
        File::open(&self.path)
            .map_err(io_error)?
            .take((MAX_CANONICAL_HISTORY_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(io_error)?;
        if bytes.len() > MAX_CANONICAL_HISTORY_BYTES {
            return Err(invalid("canonical file size exceeded"));
        }
        #[derive(serde::Deserialize)]
        struct Version {
            #[serde(default)]
            format_version: u16,
        }
        let version: Version =
            serde_json::from_slice(&bytes).map_err(|_| invalid("invalid canonical file"))?;
        if version.format_version != 1 {
            return Err(invalid("NO MORE SUPPORTED PLEASE UPDATE"));
        }
        let history: History =
            serde_json::from_slice(&bytes).map_err(|_| invalid("invalid canonical file"))?;
        history.validate(&self.stream)?;
        Ok(history)
    }
}

impl CanonicalHistoryTransactionV1<'_> {
    /// Persists an authenticated batch while retaining the verified-head lock.
    /// Writer use additionally requires the host's current, exact, atomic authority fence.
    pub fn append_authenticated(
        &mut self,
        lease: &ServiceLeaderLease,
        message: SyncMessage,
    ) -> SyncResult<CanonicalHeadV1> {
        if self.failed {
            return Err(invalid(
                "canonical transaction requires reopen after failure",
            ));
        }
        if CanonicalStreamV1::from_lease(lease)? != self.history.stream
            || lease.epoch == 0
            || lease.expires_at_ms <= lease.acquired_at_ms
        {
            return Err(invalid("canonical grant scope mismatch"));
        }
        if let Some(existing) = self
            .history
            .batches
            .iter()
            .find(|old| old.message.batch_id == message.batch_id)
        {
            return if existing.message == message
                && existing.epoch == lease.epoch
                && existing.writer == lease.holder_core_id.as_str()
            {
                Ok(self.history.head())
            } else {
                Err(invalid("divergent canonical replay"))
            };
        }
        let mut batch = Batch {
            epoch: lease.epoch,
            writer: lease.holder_core_id.as_str().into(),
            message,
            digest: String::new(),
        };
        batch.digest = digest(&self.history.stream, self.history.batches.last(), &batch)?;
        self.append_batch(batch)
    }

    fn append_batch(&mut self, batch: Batch) -> SyncResult<CanonicalHeadV1> {
        if self.failed {
            return Err(invalid(
                "canonical transaction requires reopen after failure",
            ));
        }
        if let Some(existing) = self
            .history
            .batches
            .iter()
            .find(|old| old.message.batch_id == batch.message.batch_id)
        {
            return if existing == &batch {
                Ok(self.history.head())
            } else {
                Err(invalid("divergent canonical replay"))
            };
        }
        self.history.batches.push(batch);
        let result = self
            .history
            .validate(&self.history.stream)
            .and_then(|()| save(self.path, &self.history));
        if let Err(error) = result {
            self.failed = true;
            self.history.batches.pop();
            return Err(error);
        }
        Ok(self.history.head())
    }
}

fn target_at_head(history: &History, required: &CanonicalHeadV1) -> SyncResult<DurabilityTargetV1> {
    if required.stream != history.stream {
        return Err(invalid("durability stream mismatch"));
    }
    let batch = history
        .batches
        .iter()
        .find(|batch| batch.message.sequence_end == required.sequence)
        .ok_or(invalid("durability target not retained"))?;
    if required.batch_hash.as_deref() != Some(batch.message.events_hash.as_str())
        || required.history_hash.as_deref() != Some(batch.digest.as_str())
    {
        return Err(invalid("durability target head mismatch"));
    }
    Ok(DurabilityTargetV1 {
        batch_id: batch.message.batch_id.clone(),
        head: required.clone(),
        writer_epoch: batch.epoch,
        writer_core_id: batch.writer.clone(),
    })
}

fn save(path: &Path, history: &History) -> SyncResult<()> {
    let bytes = serde_json::to_vec(history).map_err(|_| invalid("canonical encoding failed"))?;
    if bytes.len() > MAX_CANONICAL_HISTORY_BYTES {
        return Err(invalid("canonical file size exceeded"));
    }
    atomic_write(path, &bytes)
}

fn io_error(error: std::io::Error) -> crate::sync::SyncError {
    crate::sync::SyncError::ReplicationFailed(error.to_string())
}
