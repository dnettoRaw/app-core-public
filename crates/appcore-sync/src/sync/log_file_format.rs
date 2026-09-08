// =============================================================================
//        #######
//     ###       ###     F: log_file_format.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/02 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/02 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Incremental scanner and direct writer for replication-log V1 records.

use crate::sync::codec::hex_to_bytes;
use crate::sync::error::{SyncError, SyncResult, UPDATE_REQUIRED_MESSAGE};
use crate::sync::log::{
    replication_record_hash, validate_record_size, MAX_REPLICATION_LOG_BYTES,
    MAX_REPLICATION_RECORDS, MAX_REPLICATION_RECORD_BYTES, REPLICATION_LOG_FORMAT_V1,
};
use crate::sync::persistence::reject_symlink;
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;

const MAX_LINE_BYTES: usize = MAX_REPLICATION_RECORD_BYTES * 2 + 160;
const HEX: &[u8; 16] = b"0123456789abcdef";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RecordLocation {
    pub(super) sequence: u64,
    pub(super) payload_offset: u64,
    pub(super) payload_bytes: u32,
    pub(super) hash_offset: u64,
    payload_digest: [u8; 32],
}

pub(super) struct LogHeader {
    pub(super) body_offset: u64,
    pub(super) file_bytes: u64,
    pub(super) incomplete: bool,
}

pub(super) struct TailScan {
    pub(super) locations: Vec<RecordLocation>,
    pub(super) scanned_bytes: u64,
    pub(super) record_count: usize,
    pub(super) line_count: usize,
    pub(super) chain_head: String,
    pub(super) recovered_tail: bool,
}

enum LineStatus {
    Complete,
    Partial,
    End,
}

pub(super) fn read_header(path: &Path) -> SyncResult<LogHeader> {
    reject_symlink(path)?;
    let file = File::open(path).map_err(io_error)?;
    let file_bytes = file.metadata().map_err(io_error)?.len();
    if file_bytes > MAX_REPLICATION_LOG_BYTES {
        return Err(log_full());
    }
    let mut reader = BufReader::new(file);
    let mut line = Vec::new();
    let (status, consumed) =
        read_bounded_line(&mut reader, &mut line, REPLICATION_LOG_FORMAT_V1.len())?;
    if matches!(status, LineStatus::End) || line != REPLICATION_LOG_FORMAT_V1.as_bytes() {
        return Err(SyncError::ReplicationFailed(
            UPDATE_REQUIRED_MESSAGE.to_string(),
        ));
    }
    Ok(LogHeader {
        body_offset: consumed as u64,
        file_bytes,
        incomplete: matches!(status, LineStatus::Partial),
    })
}

pub(super) fn scan_tail(
    path: &Path,
    start: u64,
    record_count: usize,
    line_count: usize,
    chain_head: &str,
    existing: &[RecordLocation],
    sequences: &[(u64, usize)],
) -> SyncResult<TailScan> {
    reject_symlink(path)?;
    let file = File::open(path).map_err(io_error)?;
    let file_bytes = file.metadata().map_err(io_error)?.len();
    if file_bytes > MAX_REPLICATION_LOG_BYTES || start > file_bytes {
        return Err(log_full());
    }
    let mut buffered = BufReader::new(file);
    buffered.seek(SeekFrom::Start(start)).map_err(io_error)?;
    let mut reader = buffered.take(file_bytes - start);
    let mut line = Vec::new();
    let mut offset = start;
    let mut records = record_count;
    let mut lines = line_count;
    let mut previous = chain_head.to_string();
    let mut locations = Vec::new();
    let mut appended_sequences = Vec::new();
    let mut recovered_tail = false;
    loop {
        let line_start = offset;
        let (status, consumed) = read_bounded_line(&mut reader, &mut line, MAX_LINE_BYTES)?;
        match status {
            LineStatus::End => break,
            LineStatus::Partial => {
                recovered_tail = true;
                offset = line_start;
                break;
            }
            LineStatus::Complete => {
                offset = offset.saturating_add(consumed as u64);
                lines = lines.saturating_add(1);
                let context = ParseContext {
                    path,
                    line_number: lines,
                    previous_hash: &previous,
                    existing,
                    sequences,
                    appended: &locations,
                    appended_sequences: &appended_sequences,
                };
                if let Some(location) = parse_line(&line, line_start, &context)? {
                    records = records
                        .checked_add(1)
                        .filter(|count| *count <= MAX_REPLICATION_RECORDS)
                        .ok_or_else(|| replication_failure("replication record limit exceeded"))?;
                    previous = record_hash_field(&line)?.to_string();
                    if location.sequence > 0 {
                        insert_sequence(
                            &mut appended_sequences,
                            location.sequence,
                            locations.len(),
                        );
                    }
                    locations.push(location);
                }
            }
        }
    }
    Ok(TailScan {
        locations,
        scanned_bytes: offset,
        record_count: records,
        line_count: lines,
        chain_head: previous,
        recovered_tail,
    })
}

pub(super) fn read_payload(path: &Path, location: &RecordLocation) -> SyncResult<Vec<u8>> {
    reject_symlink(path)?;
    let mut file = File::open(path).map_err(io_error)?;
    read_payload_from(&mut file, location)
}

pub(super) fn read_payload_from(file: &mut File, location: &RecordLocation) -> SyncResult<Vec<u8>> {
    let payload_bytes = location.payload_bytes as usize;
    if payload_bytes > MAX_REPLICATION_RECORD_BYTES {
        return Err(replication_failure("indexed payload exceeds size limit"));
    }
    file.seek(SeekFrom::Start(location.payload_offset))
        .map_err(io_error)?;
    let mut payload = Vec::with_capacity(payload_bytes);
    let mut encoded = [0u8; 8192];
    let mut remaining = payload_bytes.saturating_mul(2);
    let mut hasher = Sha256::new();
    while remaining > 0 {
        let take = remaining.min(encoded.len());
        file.read_exact(&mut encoded[..take]).map_err(io_error)?;
        for pair in encoded[..take].chunks_exact(2) {
            let byte = (hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?;
            payload.push(byte);
            hasher.update([byte]);
        }
        remaining -= take;
    }
    if hasher.finalize().as_slice() != location.payload_digest {
        return Err(replication_failure("indexed payload digest mismatch"));
    }
    Ok(payload)
}

pub(super) fn anchor_matches(
    path: &Path,
    location: Option<&RecordLocation>,
    chain_head: &str,
) -> SyncResult<bool> {
    let Some(location) = location else {
        return Ok(true);
    };
    let mut file = File::open(path).map_err(io_error)?;
    file.seek(SeekFrom::Start(location.hash_offset))
        .map_err(io_error)?;
    let mut encoded = [0u8; 64];
    if file.read_exact(&mut encoded).is_err() {
        return Ok(false);
    }
    Ok(encoded == chain_head.as_bytes())
}

pub(super) fn append_record(
    path: &Path,
    expected_offset: u64,
    sequence: u64,
    payload: &[u8],
    previous_hash: &str,
    record_hash: &str,
) -> SyncResult<(RecordLocation, u64)> {
    reject_symlink(path)?;
    validate_record_size(payload)?;
    let line_bytes = encoded_line_bytes(sequence, payload.len(), previous_hash.len())?;
    let next = expected_offset
        .checked_add(line_bytes)
        .filter(|bytes| *bytes <= MAX_REPLICATION_LOG_BYTES)
        .ok_or_else(log_full)?;
    let mut file = OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(io_error)?;
    if file.metadata().map_err(io_error)?.len() != expected_offset {
        return Err(replication_failure("replication log changed outside lock"));
    }
    let (location, _) = write_record(
        &mut file,
        expected_offset,
        sequence,
        payload,
        previous_hash,
        record_hash,
    )?;
    file.sync_data().map_err(io_error)?;
    Ok((location, next))
}

pub(super) fn write_record(
    writer: &mut impl Write,
    offset: u64,
    sequence: u64,
    payload: &[u8],
    previous_hash: &str,
    record_hash: &str,
) -> SyncResult<(RecordLocation, u64)> {
    validate_record_size(payload)?;
    let sequence_text = sequence.to_string();
    let payload_offset = offset + sequence_text.len() as u64 + 1;
    writer
        .write_all(sequence_text.as_bytes())
        .map_err(io_error)?;
    writer.write_all(b"\t").map_err(io_error)?;
    write_hex(writer, payload)?;
    writer.write_all(b"\t").map_err(io_error)?;
    writer
        .write_all(previous_hash.as_bytes())
        .map_err(io_error)?;
    writer.write_all(b"\t").map_err(io_error)?;
    let hash_offset = payload_offset + (payload.len() * 2 + 1 + previous_hash.len() + 1) as u64;
    writer.write_all(record_hash.as_bytes()).map_err(io_error)?;
    writer.write_all(b"\n").map_err(io_error)?;
    let line_bytes = encoded_line_bytes(sequence, payload.len(), previous_hash.len())?;
    Ok((
        RecordLocation {
            sequence,
            payload_offset,
            payload_bytes: u32::try_from(payload.len())
                .map_err(|_| replication_failure("replication payload length overflow"))?,
            hash_offset,
            payload_digest: Sha256::digest(payload).into(),
        },
        line_bytes,
    ))
}

struct ParseContext<'a> {
    path: &'a Path,
    line_number: usize,
    previous_hash: &'a str,
    existing: &'a [RecordLocation],
    sequences: &'a [(u64, usize)],
    appended: &'a [RecordLocation],
    appended_sequences: &'a [(u64, usize)],
}

fn parse_line(
    line: &[u8],
    offset: u64,
    context: &ParseContext<'_>,
) -> SyncResult<Option<RecordLocation>> {
    if line.iter().all(u8::is_ascii_whitespace) {
        return Ok(None);
    }
    let tabs = tab_positions(line, context.line_number)?;
    let sequence_text = field_text(&line[..tabs[0]], context.line_number, "invalid sequence")?;
    let sequence = sequence_text
        .parse::<u64>()
        .map_err(|_| corrupt(context.line_number, "invalid sequence"))?;
    let payload_hex = &line[tabs[0] + 1..tabs[1]];
    if payload_hex.len() > MAX_REPLICATION_RECORD_BYTES * 2 || !payload_hex.len().is_multiple_of(2)
    {
        return Err(corrupt(context.line_number, "invalid event hex"));
    }
    let payload = hex_to_bytes(field_text(
        payload_hex,
        context.line_number,
        "invalid event hex",
    )?)
    .map_err(|_| corrupt(context.line_number, "invalid event hex"))?;
    validate_record_size(&payload)?;
    let stored_previous = field_text(
        &line[tabs[1] + 1..tabs[2]],
        context.line_number,
        "hash chain mismatch",
    )?;
    let stored_hash = field_text(
        &line[tabs[2] + 1..],
        context.line_number,
        "hash chain mismatch",
    )?;
    let computed = replication_record_hash(context.previous_hash, sequence, &payload);
    if stored_previous != context.previous_hash || stored_hash != computed {
        return Err(corrupt(context.line_number, "hash chain mismatch"));
    }
    validate_sequence(context, sequence, &payload)?;
    Ok(Some(RecordLocation {
        sequence,
        payload_offset: offset + tabs[0] as u64 + 1,
        payload_bytes: u32::try_from(payload.len())
            .map_err(|_| replication_failure("replication payload length overflow"))?,
        hash_offset: offset + tabs[2] as u64 + 1,
        payload_digest: Sha256::digest(&payload).into(),
    }))
}

fn validate_sequence(context: &ParseContext<'_>, sequence: u64, payload: &[u8]) -> SyncResult<()> {
    if sequence == 0 {
        return Ok(());
    }
    let location = context
        .sequences
        .binary_search_by_key(&sequence, |(key, _)| *key)
        .ok()
        .and_then(|position| context.existing.get(context.sequences[position].1))
        .or_else(|| {
            context
                .appended_sequences
                .binary_search_by_key(&sequence, |(key, _)| *key)
                .ok()
                .and_then(|position| context.appended.get(context.appended_sequences[position].1))
        });
    let Some(location) = location else {
        return Ok(());
    };
    if read_payload(context.path, location)? != payload {
        return Err(SyncError::SequenceConflict(sequence));
    }
    Err(corrupt(context.line_number, "duplicate sequence"))
}

fn insert_sequence(index: &mut Vec<(u64, usize)>, sequence: u64, record_index: usize) {
    match index.binary_search_by_key(&sequence, |(key, _)| *key) {
        Ok(position) => index[position] = (sequence, record_index),
        Err(position) => index.insert(position, (sequence, record_index)),
    }
}

fn record_hash_field(line: &[u8]) -> SyncResult<&str> {
    let position = line
        .iter()
        .rposition(|byte| *byte == b'\t')
        .ok_or_else(|| corrupt(1, "missing separator"))?;
    field_text(&line[position + 1..], 1, "hash chain mismatch")
}

fn tab_positions(line: &[u8], line_number: usize) -> SyncResult<[usize; 3]> {
    let mut positions = [0usize; 3];
    let mut count = 0usize;
    for (index, byte) in line.iter().enumerate() {
        if *byte == b'\t' {
            if count == positions.len() {
                return Err(corrupt(line_number, "invalid field count"));
            }
            positions[count] = index;
            count += 1;
        }
    }
    if count != positions.len() {
        let reason = if count == 0 {
            "missing separator"
        } else {
            "invalid field count"
        };
        return Err(corrupt(line_number, reason));
    }
    Ok(positions)
}

fn read_bounded_line(
    reader: &mut impl BufRead,
    line: &mut Vec<u8>,
    max_bytes: usize,
) -> SyncResult<(LineStatus, usize)> {
    line.clear();
    let mut consumed = 0usize;
    loop {
        let available = reader.fill_buf().map_err(io_error)?;
        if available.is_empty() {
            let status = if line.is_empty() {
                LineStatus::End
            } else {
                LineStatus::Partial
            };
            return Ok((status, consumed));
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let take = newline.map_or(available.len(), |index| index + 1);
        let body = newline.unwrap_or(available.len());
        if line.len().saturating_add(body) > max_bytes {
            return Err(replication_failure("replication record exceeds size limit"));
        }
        line.extend_from_slice(&available[..body]);
        reader.consume(take);
        consumed = consumed.saturating_add(take);
        if newline.is_some() {
            return Ok((LineStatus::Complete, consumed));
        }
    }
}

fn write_hex(writer: &mut impl Write, payload: &[u8]) -> SyncResult<()> {
    let mut buffer = [0u8; 8192];
    for chunk in payload.chunks(buffer.len() / 2) {
        for (index, byte) in chunk.iter().enumerate() {
            buffer[index * 2] = HEX[(byte >> 4) as usize];
            buffer[index * 2 + 1] = HEX[(byte & 0x0f) as usize];
        }
        writer
            .write_all(&buffer[..chunk.len() * 2])
            .map_err(io_error)?;
    }
    Ok(())
}

fn encoded_line_bytes(
    sequence: u64,
    payload_bytes: usize,
    previous_bytes: usize,
) -> SyncResult<u64> {
    let digits = sequence.to_string().len() as u64;
    digits
        .checked_add(1 + (payload_bytes as u64) * 2 + 1 + previous_bytes as u64 + 1 + 64 + 1)
        .ok_or_else(|| replication_failure("replication line length overflow"))
}

fn field_text<'a>(bytes: &'a [u8], line: usize, reason: &'static str) -> SyncResult<&'a str> {
    std::str::from_utf8(bytes).map_err(|_| corrupt(line, reason))
}

fn hex_nibble(byte: u8) -> SyncResult<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(replication_failure("indexed payload is not hexadecimal")),
    }
}

fn corrupt(line: usize, reason: &'static str) -> SyncError {
    SyncError::CorruptReplicationLog { line, reason }
}

fn log_full() -> SyncError {
    replication_failure("replication log exceeds configured limit")
}

fn replication_failure(message: &str) -> SyncError {
    SyncError::ReplicationFailed(message.to_string())
}

fn io_error(error: std::io::Error) -> SyncError {
    SyncError::ReplicationFailed(error.to_string())
}
