// =============================================================================
//        #######
//     ###       ###     F: idempotency_encoding.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/02 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/02 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Counts and fingerprints one idempotency record without materializing JSON.

use crate::error::{RuntimeError, RuntimeResult};
use crate::idempotency::IdempotencyRecord;
use sha2::{Digest, Sha256};
use std::io::Write;

pub(crate) const MAX_IDEMPOTENCY_RECORD_BYTES: usize = 1024 * 1024;

pub(crate) struct RecordEncoding {
    pub(crate) bytes: u64,
    pub(crate) digest: [u8; 32],
}

pub(crate) fn measure_record(record: &IdempotencyRecord) -> RuntimeResult<RecordEncoding> {
    let mut counter = LimitedCounter::new(MAX_IDEMPOTENCY_RECORD_BYTES as u64);
    let result = serde_json::to_writer(&mut counter, record);
    if counter.exceeded {
        return Err(validation_error("record exceeds size limit"));
    }
    result.map_err(serialization_error)?;
    Ok(RecordEncoding {
        bytes: counter.bytes,
        digest: counter.hasher.finalize().into(),
    })
}

struct LimitedCounter {
    bytes: u64,
    limit: u64,
    exceeded: bool,
    hasher: Sha256,
}

impl LimitedCounter {
    fn new(limit: u64) -> Self {
        Self {
            bytes: 0,
            limit,
            exceeded: false,
            hasher: Sha256::new(),
        }
    }
}

impl Write for LimitedCounter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let Some(next) = self.bytes.checked_add(buffer.len() as u64) else {
            self.exceeded = true;
            return Err(std::io::Error::other("record size overflow"));
        };
        if next > self.limit {
            self.exceeded = true;
            return Err(std::io::Error::other("record exceeds size limit"));
        }
        self.bytes = next;
        self.hasher.update(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn serialization_error(error: serde_json::Error) -> RuntimeError {
    RuntimeError::IdempotencyStoreIo {
        operation: "serialize_store_entry",
        message: error.to_string(),
    }
}

fn validation_error(message: &str) -> RuntimeError {
    RuntimeError::IdempotencyStoreIo {
        operation: "validate_store",
        message: message.to_string(),
    }
}
