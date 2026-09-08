// =============================================================================
//        #######
//     ###       ###     F: message_json.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/03 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/03 00:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Writes canonical synchronization message JSON to bounded sinks.

use crate::sync::SyncMessage;
use std::io::{self, Write};

const EVENT_BUFFER_BYTES: usize = 16 * 1024;

/// Writes the compact JSON representation used by `SyncMessage` serialization.
///
/// Unlike `serde_json::to_vec`, this function does not require a second
/// payload-sized allocation. It preserves the derived Serde field order and
/// escaping so persistent providers can stream directly to bounded storage.
pub fn write_sync_message_json(writer: &mut impl Write, message: &SyncMessage) -> io::Result<()> {
    writer.write_all(b"{\"batch_id\":")?;
    write_string(writer, &message.batch_id)?;
    writer.write_all(b",\"source_node_id\":")?;
    write_string(writer, message.source_node_id.as_str())?;
    writer.write_all(b",\"sequence_start\":")?;
    write_u64(writer, message.sequence_start)?;
    writer.write_all(b",\"sequence_end\":")?;
    write_u64(writer, message.sequence_end)?;
    writer.write_all(b",\"event_count\":")?;
    write_usize(writer, message.event_count)?;
    writer.write_all(b",\"events_hash\":")?;
    write_string(writer, &message.events_hash)?;
    writer.write_all(b",\"created_at_ms\":")?;
    write_u64(writer, message.created_at_ms)?;
    writer.write_all(b",\"previous_batch_hash\":")?;
    match message.previous_batch_hash.as_deref() {
        Some(hash) => write_string(writer, hash)?,
        None => writer.write_all(b"null")?,
    }
    writer.write_all(b",\"events\":[")?;
    write_events(writer, &message.events)?;
    writer.write_all(b"]}")
}

fn write_events(writer: &mut impl Write, events: &[Vec<u8>]) -> io::Result<()> {
    let mut output = EventJsonWriter::new(writer);
    for (event_index, event) in events.iter().enumerate() {
        if event_index != 0 {
            output.push(b',')?;
        }
        output.push(b'[')?;
        for (byte_index, value) in event.iter().enumerate() {
            if byte_index != 0 {
                output.push(b',')?;
            }
            output.push_byte(*value)?;
        }
        output.push(b']')?;
    }
    output.finish()
}

fn write_string(writer: &mut impl Write, value: &str) -> io::Result<()> {
    serde_json::to_writer(writer, value).map_err(io::Error::other)
}

fn write_u64(writer: &mut impl Write, value: u64) -> io::Result<()> {
    write_decimal(writer, u128::from(value))
}

fn write_usize(writer: &mut impl Write, value: usize) -> io::Result<()> {
    write_decimal(writer, value as u128)
}

fn write_decimal(writer: &mut impl Write, mut value: u128) -> io::Result<()> {
    let mut buffer = [0u8; 39];
    let mut cursor = buffer.len();
    loop {
        cursor -= 1;
        buffer[cursor] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            return writer.write_all(&buffer[cursor..]);
        }
    }
}

struct EventJsonWriter<'a, W> {
    writer: &'a mut W,
    buffer: [u8; EVENT_BUFFER_BYTES],
    used: usize,
}

impl<'a, W: Write> EventJsonWriter<'a, W> {
    fn new(writer: &'a mut W) -> Self {
        Self {
            writer,
            buffer: [0; EVENT_BUFFER_BYTES],
            used: 0,
        }
    }

    fn push(&mut self, byte: u8) -> io::Result<()> {
        if self.used == self.buffer.len() {
            self.flush()?;
        }
        self.buffer[self.used] = byte;
        self.used += 1;
        Ok(())
    }

    fn push_byte(&mut self, value: u8) -> io::Result<()> {
        let hundreds = value / 100;
        let tens = (value % 100) / 10;
        let ones = value % 10;
        if hundreds != 0 {
            self.push(hundreds + b'0')?;
        }
        if hundreds != 0 || tens != 0 {
            self.push(tens + b'0')?;
        }
        self.push(ones + b'0')
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.write_all(&self.buffer[..self.used])?;
        self.used = 0;
        Ok(())
    }

    fn finish(mut self) -> io::Result<()> {
        self.flush()
    }
}
