// =============================================================================
//        #######
//     ###       ###     F: state_file_stream.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/02 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/02 00:00:00 by dnettoRaw
//      ###########      S: 1.0.0
// =============================================================================

//! Borrowed and bounded serialization for Scheduler State Provider V1.

use crate::{DurableTaskMisfirePolicyV1, SchedulerStateError, SchedulerStateRecordV1};
use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{self, BufWriter, Write};

const SERIALIZATION_BUFFER_BYTES: usize = 64 * 1024;

#[derive(Serialize)]
pub(super) struct StateFileRef<'a> {
    format: &'static str,
    records: RecordsRef<'a>,
    checksum: &'a str,
}

impl<'a> StateFileRef<'a> {
    pub(super) fn new(
        format: &'static str,
        records: &'a BTreeMap<String, SchedulerStateRecordV1>,
        checksum: &'a str,
    ) -> Self {
        Self {
            format,
            records: RecordsRef(records),
            checksum,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct RecordsRef<'a>(&'a BTreeMap<String, SchedulerStateRecordV1>);

impl<'a> RecordsRef<'a> {
    pub(super) const fn new(records: &'a BTreeMap<String, SchedulerStateRecordV1>) -> Self {
        Self(records)
    }
}

impl Serialize for RecordsRef<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for record in self.0.values() {
            sequence.serialize_element(&FileRecordRef::from(record))?;
        }
        sequence.end()
    }
}

#[derive(Serialize)]
struct FileRecordRef<'a> {
    task_id: &'a str,
    definition_hash: &'a str,
    next_run_ms: u64,
    attempts: u32,
    misfire_policy: &'static str,
    completed: bool,
    last_receipt_epoch: Option<u64>,
    claim: Option<FileClaimRef<'a>>,
    fencing_epoch: u64,
}

impl<'a> From<&'a SchedulerStateRecordV1> for FileRecordRef<'a> {
    fn from(record: &'a SchedulerStateRecordV1) -> Self {
        Self {
            task_id: &record.task_id,
            definition_hash: &record.definition_hash,
            next_run_ms: record.next_run_ms,
            attempts: record.attempts,
            misfire_policy: match record.misfire_policy {
                DurableTaskMisfirePolicyV1::FireOnce => "fire_once",
                DurableTaskMisfirePolicyV1::Skip => "skip",
            },
            completed: record.completed,
            last_receipt_epoch: record.last_receipt_epoch,
            claim: record.claim.as_ref().map(FileClaimRef::from),
            fencing_epoch: record.fencing_epoch,
        }
    }
}

#[derive(Serialize)]
struct FileClaimRef<'a> {
    task_id: &'a str,
    owner_id: &'a str,
    fencing_epoch: u64,
    lease_until_ms: u64,
    attempt: u32,
}

impl<'a> From<&'a crate::SchedulerStateClaimV1> for FileClaimRef<'a> {
    fn from(claim: &'a crate::SchedulerStateClaimV1) -> Self {
        Self {
            task_id: &claim.task_id,
            owner_id: claim.owner_id(),
            fencing_epoch: claim.fencing_epoch,
            lease_until_ms: claim.lease_until_ms,
            attempt: claim.attempt,
        }
    }
}

pub(super) fn checksum(value: &impl Serialize) -> Result<String, SchedulerStateError> {
    let mut digest = DigestWriter(Sha256::new());
    {
        let mut buffered = BufWriter::with_capacity(SERIALIZATION_BUFFER_BYTES, &mut digest);
        serde_json::to_writer(&mut buffered, value).map_err(unavailable)?;
        buffered
            .flush()
            .map_err(|_| SchedulerStateError::Unavailable)?;
    }
    Ok(hex_digest(digest.0.finalize().into()))
}

pub(super) fn write_bounded_json(
    output: &mut impl Write,
    value: &impl Serialize,
    limit: u64,
) -> Result<(), SchedulerStateError> {
    let mut bounded = BoundedWriter::new(output, limit);
    let result = {
        let mut buffered = BufWriter::with_capacity(SERIALIZATION_BUFFER_BYTES, &mut bounded);
        let result = serde_json::to_writer(&mut buffered, value);
        result.and_then(|()| buffered.flush().map_err(serde_json::Error::io))
    };
    if bounded.exceeded {
        return Err(SchedulerStateError::CapacityExceeded {
            max_records: crate::MAX_SCHEDULER_STATE_RECORDS,
        });
    }
    result.map_err(unavailable)
}

struct DigestWriter(Sha256);

impl Write for DigestWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
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
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() as u64 > self.remaining {
            self.exceeded = true;
            return Err(io::Error::other("scheduler state exceeds configured limit"));
        }
        let written = self.inner.write(bytes)?;
        self.remaining = self.remaining.saturating_sub(written as u64);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

fn hex_digest(digest: [u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn unavailable(_error: serde_json::Error) -> SchedulerStateError {
    SchedulerStateError::Unavailable
}
