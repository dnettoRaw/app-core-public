// =============================================================================
//        #######
//     ###       ###     F: operational_journal_encoding.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/02 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/02 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Streams operational journal records into counters, digests and files.

use crate::operational_journal::{
    journal_io, journal_message, OperationalJournalRecord, MAX_JOURNAL_RECORD_BYTES,
};
use crate::{RuntimeResult, OPERATIONAL_JOURNAL_FORMAT_V1};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

#[derive(Serialize)]
struct JournalEnvelopeRef<'a> {
    sequence: u64,
    previous_hash: &'a str,
    hash: &'a str,
    record: &'a OperationalJournalRecord,
}

const HASH_HEX_BYTES: u64 = 64;
const ENVELOPE_FIXED_BYTES: u64 = b"{\"sequence\":".len() as u64
    + b",\"previous_hash\":\"".len() as u64
    + b"\",\"hash\":\"".len() as u64
    + HASH_HEX_BYTES
    + b"\",\"record\":".len() as u64
    + b"}\n".len() as u64;

pub(super) fn encode_records<'a>(
    writer: &mut impl Write,
    records: impl IntoIterator<Item = &'a OperationalJournalRecord>,
) -> RuntimeResult<(u64, String)> {
    write_header(writer)?;
    let mut sequence = 0u64;
    let mut last_hash = String::new();
    for record in records {
        sequence = sequence.saturating_add(1);
        let hash = record_hash(sequence, &last_hash, record)?;
        write_envelope(
            writer,
            &JournalEnvelopeRef {
                sequence,
                previous_hash: &last_hash,
                hash: &hash,
                record,
            },
        )?;
        last_hash = hash;
    }
    Ok((sequence, last_hash))
}

pub(super) fn append_envelope(
    path: &Path,
    sequence: u64,
    previous_hash: &str,
    hash: &str,
    record: &OperationalJournalRecord,
) -> RuntimeResult<()> {
    let mut file = OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|error| journal_io("open_append", error))?;
    write_envelope(
        &mut file,
        &JournalEnvelopeRef {
            sequence,
            previous_hash,
            hash,
            record,
        },
    )?;
    file.sync_data()
        .map_err(|error| journal_io("sync_record", error))
}

pub(super) fn record_hash(
    sequence: u64,
    previous_hash: &str,
    record: &OperationalJournalRecord,
) -> RuntimeResult<String> {
    let record_bytes = measure_record(record)?;
    let mut hasher = Sha256::new();
    hasher.update(OPERATIONAL_JOURNAL_FORMAT_V1.as_bytes());
    hasher.update(sequence.to_be_bytes());
    hasher.update((previous_hash.len() as u64).to_be_bytes());
    hasher.update(previous_hash.as_bytes());
    hasher.update(record_bytes.to_be_bytes());
    serde_json::to_writer(HashWriter(&mut hasher), record)
        .map_err(|error| journal_message("serialize_record", error.to_string()))?;
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
pub(super) fn encoded_records_bytes<'a>(
    records: impl IntoIterator<Item = &'a OperationalJournalRecord>,
) -> RuntimeResult<u64> {
    let mut counter = ByteCounter::default();
    encode_records(&mut counter, records)?;
    Ok(counter.bytes)
}

pub(super) fn retained_suffix_count(
    records: &VecDeque<Arc<OperationalJournalRecord>>,
    max_bytes: u64,
) -> RuntimeResult<usize> {
    if records.is_empty() {
        return Ok(0);
    }
    let mut record_bytes = 0u64;
    let mut envelope_bytes = 0u64;
    let mut retained = 0usize;
    for record in records.iter().rev() {
        let count = retained
            .checked_add(1)
            .ok_or_else(|| journal_message("validate_size", "journal size overflow".to_string()))?;
        let count_u64 = u64::try_from(count)
            .map_err(|_| journal_message("validate_size", "journal size overflow".to_string()))?;
        record_bytes = record_bytes
            .checked_add(measure_record(record.as_ref())?)
            .ok_or_else(|| journal_message("validate_size", "journal size overflow".to_string()))?;
        envelope_bytes = envelope_bytes
            .checked_add(ENVELOPE_FIXED_BYTES)
            .and_then(|bytes| bytes.checked_add(decimal_digits(count_u64)))
            .and_then(|bytes| bytes.checked_add(if count > 1 { HASH_HEX_BYTES } else { 0 }))
            .ok_or_else(|| journal_message("validate_size", "journal size overflow".to_string()))?;
        let total = u64::try_from(OPERATIONAL_JOURNAL_FORMAT_V1.len())
            .ok()
            .and_then(|bytes| bytes.checked_add(1))
            .and_then(|bytes| bytes.checked_add(record_bytes))
            .and_then(|bytes| bytes.checked_add(envelope_bytes))
            .ok_or_else(|| journal_message("validate_size", "journal size overflow".to_string()))?;
        if total > max_bytes {
            break;
        }
        retained = count;
    }
    Ok(retained.max(1))
}

pub(super) fn write_header(writer: &mut impl Write) -> RuntimeResult<()> {
    writer
        .write_all(OPERATIONAL_JOURNAL_FORMAT_V1.as_bytes())
        .and_then(|()| writer.write_all(b"\n"))
        .map_err(|error| journal_io("write_format", error))
}

fn measure_record(record: &OperationalJournalRecord) -> RuntimeResult<u64> {
    let mut counter = LimitedCounter::new(MAX_JOURNAL_RECORD_BYTES as u64);
    let result = serde_json::to_writer(&mut counter, record);
    if counter.exceeded {
        return Err(journal_message(
            "validate_record",
            "record exceeds size limit".to_string(),
        ));
    }
    result.map_err(|error| journal_message("serialize_record", error.to_string()))?;
    Ok(counter.bytes)
}

fn decimal_digits(mut value: u64) -> u64 {
    let mut digits = 1;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}

fn write_envelope(writer: &mut impl Write, envelope: &JournalEnvelopeRef<'_>) -> RuntimeResult<()> {
    serde_json::to_writer(&mut *writer, envelope)
        .map_err(|error| journal_message("serialize_envelope", error.to_string()))?;
    writer
        .write_all(b"\n")
        .map_err(|error| journal_io("write_envelope", error))
}

#[cfg(test)]
#[derive(Default)]
struct ByteCounter {
    bytes: u64,
}

#[cfg(test)]
impl Write for ByteCounter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(buffer.len() as u64)
            .ok_or_else(|| std::io::Error::other("journal size overflow"))?;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
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
        let Some(next) = self.bytes.checked_add(buffer.len() as u64) else {
            self.exceeded = true;
            return Err(std::io::Error::other("record size overflow"));
        };
        if next > self.limit {
            self.exceeded = true;
            return Err(std::io::Error::other("record exceeds size limit"));
        }
        self.bytes = next;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct HashWriter<'a>(&'a mut Sha256);

impl Write for HashWriter<'_> {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.0.update(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
