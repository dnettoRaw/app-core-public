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
    limit: Option<usize>,
}

impl Write for CountingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let next = self
            .bytes
            .checked_add(bytes.len())
            .ok_or_else(|| io::Error::other("serialized size overflow"))?;
        if self.limit.is_some_and(|limit| next > limit) {
            return Err(io::Error::other("serialized size exceeds budget"));
        }
        self.bytes = next;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) fn serialized_size<T: Serialize>(value: &T) -> Result<usize> {
    serialized_size_bounded(value, usize::MAX)
}

pub(crate) fn serialized_size_bounded<T: Serialize>(value: &T, limit: usize) -> Result<usize> {
    let mut writer = CountingWriter {
        bytes: 0,
        limit: Some(limit),
    };
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde::ser::SerializeSeq;
    use std::cell::Cell;

    struct ObservedSequence<'a>(&'a Cell<usize>);

    impl Serialize for ObservedSequence<'_> {
        fn serialize<S: serde::Serializer>(
            &self,
            serializer: S,
        ) -> std::result::Result<S::Ok, S::Error> {
            let mut sequence = serializer.serialize_seq(Some(1000))?;
            for _ in 0..1000 {
                self.0.set(self.0.get() + 1);
                sequence.serialize_element("bounded")?;
            }
            sequence.end()
        }
    }

    #[test]
    fn counting_stops_at_budget_and_preserves_exact_boundary() {
        let visited = Cell::new(0);
        let error = serialized_size_bounded(&ObservedSequence(&visited), 20).unwrap_err();
        assert_eq!(error.code(), ErrorCode::LimitExceeded);
        assert!(visited.get() < 1000);
        let value = "é日";
        let size = serialized_size(&value).unwrap();
        assert_eq!(serialized_size_bounded(&value, size).unwrap(), size);
        assert!(serialized_size_bounded(&value, size - 1).is_err());
    }
}
