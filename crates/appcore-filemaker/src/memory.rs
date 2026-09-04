// =============================================================================
//        #######
//     ###       ###     F: memory.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines allocation-free memory accounting helpers for this crate.

use std::io::{self, Write};

use serde::Serialize;

use crate::{ErrorCode, FileMakerError, Result};

#[derive(Default)]
struct CountingWriter {
    bytes: usize,
}

impl Write for CountingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .ok_or_else(|| io::Error::other("serialized size overflow"))?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) fn serialized_size<T: Serialize>(value: &T) -> Result<usize> {
    let mut writer = CountingWriter::default();
    serde_json::to_writer(&mut writer, value).map_err(|error| {
        FileMakerError::new(
            ErrorCode::LimitExceeded,
            format!("cannot account retained value: {error}"),
        )
    })?;
    Ok(writer.bytes)
}

pub(crate) fn serialized_size_pretty<T: Serialize>(value: &T) -> Result<usize> {
    let mut writer = CountingWriter::default();
    serde_json::to_writer_pretty(&mut writer, value).map_err(|error| {
        FileMakerError::new(
            ErrorCode::LimitExceeded,
            format!("cannot account retained value: {error}"),
        )
    })?;
    Ok(writer.bytes)
}
