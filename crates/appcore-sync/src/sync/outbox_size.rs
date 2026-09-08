// =============================================================================
//        #######
//     ###       ###     F: outbox_size.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/03 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/03 00:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Computes canonical JSON sizes without materializing replication payloads.

use crate::sync::{SyncError, SyncMessage, SyncResult};
use std::io::{self, Write};

const LENGTH_OVERFLOW: SyncError =
    SyncError::InvalidSyncMessage("outbox serialization length overflow");

/// Returns the exact compact JSON byte length of a synchronization message.
///
/// Event bytes are counted directly so callers can enforce storage and page
/// limits without allocating or serializing a second payload-sized buffer.
pub fn encoded_sync_message_bytes(message: &SyncMessage) -> SyncResult<usize> {
    let mut bytes = 0usize;
    add(&mut bytes, b"{\"batch_id\":".len())?;
    add(&mut bytes, encoded_string_bytes(&message.batch_id)?)?;
    add(&mut bytes, b",\"source_node_id\":".len())?;
    add(
        &mut bytes,
        encoded_string_bytes(message.source_node_id.as_str())?,
    )?;
    add(&mut bytes, b",\"sequence_start\":".len())?;
    add(&mut bytes, decimal_digits_u64(message.sequence_start))?;
    add(&mut bytes, b",\"sequence_end\":".len())?;
    add(&mut bytes, decimal_digits_u64(message.sequence_end))?;
    add(&mut bytes, b",\"event_count\":".len())?;
    add(&mut bytes, decimal_digits_usize(message.event_count))?;
    add(&mut bytes, b",\"events_hash\":".len())?;
    add(&mut bytes, encoded_string_bytes(&message.events_hash)?)?;
    add(&mut bytes, b",\"created_at_ms\":".len())?;
    add(&mut bytes, decimal_digits_u64(message.created_at_ms))?;
    add(&mut bytes, b",\"previous_batch_hash\":".len())?;
    add_optional_string(&mut bytes, message.previous_batch_hash.as_deref())?;
    add(&mut bytes, b",\"events\":[".len())?;
    add_events(&mut bytes, &message.events)?;
    add(&mut bytes, b"]}".len())?;
    Ok(bytes)
}

fn add_events(total: &mut usize, events: &[Vec<u8>]) -> SyncResult<()> {
    add(total, events.len().saturating_sub(1))?;
    for event in events {
        add(total, 2)?;
        add(total, event.len().saturating_sub(1))?;
        let digits = event.iter().try_fold(0usize, |bytes, value| {
            bytes
                .checked_add(decimal_digits_byte(*value))
                .ok_or(LENGTH_OVERFLOW)
        })?;
        add(total, digits)?;
    }
    Ok(())
}

fn add_optional_string(total: &mut usize, value: Option<&str>) -> SyncResult<()> {
    match value {
        Some(value) => add(total, encoded_string_bytes(value)?),
        None => add(total, b"null".len()),
    }
}

fn encoded_string_bytes(value: &str) -> SyncResult<usize> {
    let mut counter = JsonLengthCounter::default();
    match serde_json::to_writer(&mut counter, value) {
        Ok(()) => Ok(counter.bytes),
        Err(_) if counter.overflowed => Err(LENGTH_OVERFLOW),
        Err(_) => Err(SyncError::InvalidSyncMessage("outbox serialization failed")),
    }
}

fn add(total: &mut usize, bytes: usize) -> SyncResult<()> {
    *total = total.checked_add(bytes).ok_or(LENGTH_OVERFLOW)?;
    Ok(())
}

const fn decimal_digits_byte(value: u8) -> usize {
    if value < 10 {
        1
    } else if value < 100 {
        2
    } else {
        3
    }
}

const fn decimal_digits_u64(mut value: u64) -> usize {
    let mut digits = 1;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}

const fn decimal_digits_usize(mut value: usize) -> usize {
    let mut digits = 1;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}

#[derive(Default)]
struct JsonLengthCounter {
    bytes: usize,
    overflowed: bool,
}

impl Write for JsonLengthCounter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(total) = self.bytes.checked_add(bytes.len()) else {
            self.overflowed = true;
            return Err(io::Error::other("JSON length overflow"));
        };
        self.bytes = total;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
