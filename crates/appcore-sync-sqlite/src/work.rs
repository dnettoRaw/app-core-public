// =============================================================================
//        #######
//     ###       ###     F: work.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/12 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/12 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! One explicit `SQLite` transaction across log, outbox and checkpoints.
use crate::{SqliteSyncError, SqliteSyncResult, SqliteSyncStore};
use appcore_sync::SyncMessage;
use rusqlite::{Transaction, TransactionBehavior};

/// Bounded provider-owned unit of work; never exposes SQL or a connection.
/// Callbacks must be short and must not reenter this store or wait for network I/O.
/// Filesystem writes are outside this transaction and require a recovery journal.
pub struct SqliteSyncWork<'a> {
    transaction: &'a Transaction<'a>,
    config: &'a crate::SqliteSyncConfig,
    operations: usize,
}
impl SqliteSyncStore {
    /// Commits all operations only when the callback succeeds. An error or unwind
    /// drops the transaction and rolls back every operation. At most 1,024
    /// operations may be submitted. The deployment owns callback execution bounds.
    pub fn transaction<T>(
        &self,
        action: impl FnOnce(&mut SqliteSyncWork<'_>) -> SqliteSyncResult<T>,
    ) -> SqliteSyncResult<T> {
        self.with_connection(|connection| {
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(SqliteSyncError::database)?;
            let result = action(&mut SqliteSyncWork {
                transaction: &transaction,
                config: self.config(),
                operations: 0,
            })?;
            transaction.commit().map_err(SqliteSyncError::database)?;
            Ok(result)
        })
    }
}
impl SqliteSyncWork<'_> {
    /// Appends an opaque record. Identical positive sequence replay is a no-op.
    pub fn append(&mut self, payload: &[u8], sequence: u64) -> SqliteSyncResult<usize> {
        self.admit()?;
        crate::log::append_in_transaction(self.transaction, payload, sequence)
    }
    /// Enqueues one bounded message. Identical pending replay returns false;
    /// a full queue returns an error, ensuring the whole unit of work rolls back.
    pub fn enqueue(&mut self, message: &SyncMessage) -> SqliteSyncResult<bool> {
        self.admit()?;
        crate::outbox::validate_batch_id(&message.batch_id)
            .map_err(|_| SqliteSyncError::CorruptRecord("outbox identity"))?;
        let bytes =
            crate::outbox_blob::encoded_message_bytes(message, self.config.max_outbox_record_bytes)
                .map_err(|_| SqliteSyncError::CapacityExceeded("outbox record"))?;
        let inserted = crate::outbox::enqueue_in_transaction(
            self.transaction,
            message,
            bytes,
            self.config.max_outbox_entries,
        )?;
        if !inserted {
            let exists: bool = self
                .transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM appcore_sync_outbox WHERE batch_id = ?1)",
                    [&message.batch_id],
                    |row| row.get(0),
                )
                .map_err(SqliteSyncError::database)?;
            if !exists {
                return Err(SqliteSyncError::CapacityExceeded("outbox"));
            }
        }
        Ok(inserted)
    }
    /// Advances a checkpoint only within an authenticated observed history head.
    /// Head authenticity and materialization are the caller's responsibility.
    pub fn checkpoint(
        &mut self,
        peer: &str,
        sequence: u64,
        hash: &str,
        observed_head: u64,
    ) -> SqliteSyncResult<()> {
        self.admit()?;
        appcore_sync::validate_checkpoint_head_v1(sequence, observed_head)
            .map_err(|_| SqliteSyncError::CorruptRecord("future checkpoint"))?;
        crate::checkpoint::validate_peer_id(peer)
            .and_then(|()| crate::checkpoint::validate_hash(hash))
            .map_err(|_| SqliteSyncError::CorruptRecord("checkpoint"))?;
        let sequence =
            i64::try_from(sequence).map_err(|_| SqliteSyncError::CapacityExceeded("checkpoint"))?;
        crate::checkpoint::set_in_transaction(
            self.transaction,
            peer,
            sequence,
            hash,
            self.config.max_checkpoints,
        )
    }
    /// Journals opaque bytes before filesystem publication in the same transaction
    /// as enqueue/log/checkpoint. Replay materializes committed intents only.
    pub fn stage_object_write(
        &mut self,
        intent: &crate::ObjectWriteIntentV1,
    ) -> SqliteSyncResult<bool> {
        self.admit()?;
        crate::materialization::stage(self.transaction, intent)
    }
    /// Applies an exact ordered outbox receipt in this transaction, so checkpoint
    /// advancement and acknowledgement may share one commit boundary.
    pub fn acknowledge(
        &mut self,
        receipt: &appcore_sync::SyncOutboxReceipt,
    ) -> SqliteSyncResult<usize> {
        self.admit()?;
        crate::outbox::acknowledge_in_transaction(self.transaction, receipt)
    }
    fn admit(&mut self) -> SqliteSyncResult<()> {
        if self.operations >= 1_024 {
            return Err(SqliteSyncError::CapacityExceeded("transaction operation"));
        }
        self.operations += 1;
        Ok(())
    }
}
