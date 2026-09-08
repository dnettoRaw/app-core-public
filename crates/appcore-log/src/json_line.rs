// =============================================================================
//        #######
//     ###       ###     F: json_line.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: working-tree by dnettoRaw
//    ##   ## ##   ##    U: working-tree by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Size JSONL without allocating before reserving its exact serialized payload.

use crate::{LogError, LogEvent};
use std::io::{self, Write};

pub(crate) fn encode(event: &LogEvent, max_bytes: u64) -> Result<Vec<u8>, LogError> {
    let mut counter = Counter {
        remaining: max_bytes.checked_sub(1).ok_or(LogError::Capacity)?,
        bytes: 0,
        exceeded: false,
    };
    serde_json::to_writer(&mut counter, event).map_err(|_| {
        if counter.exceeded {
            LogError::Capacity
        } else {
            LogError::Serialization
        }
    })?;
    let size = usize::try_from(counter.bytes)
        .ok()
        .and_then(|bytes| bytes.checked_add(1))
        .ok_or(LogError::Capacity)?;
    let mut line = Vec::new();
    line.try_reserve_exact(size)
        .map_err(|_| LogError::Capacity)?;
    // LogEvent is immutable and has derived serialization: the second pass
    // produces the same bytes. The reservation includes the JSONL terminator.
    serde_json::to_writer(&mut line, event).map_err(|_| LogError::Serialization)?;
    line.push(b'\n');
    Ok(line)
}

struct Counter {
    remaining: u64,
    bytes: u64,
    exceeded: bool,
}

impl Write for Counter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let size = buffer.len() as u64;
        if size > self.remaining {
            self.exceeded = true;
            return Err(io::Error::from(io::ErrorKind::WriteZero));
        }
        self.remaining -= size;
        self.bytes += size;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Severity, Verbosity};

    #[test]
    fn exact_jsonl_budget_counts_escaping_unicode_and_newline() {
        let event = LogEvent::new(
            0,
            Severity::Info,
            Verbosity::V4,
            "test",
            "\0\n\"\\é日本語العربية",
        )
        .field("key", "\u{1}");
        let mut expected = serde_json::to_vec(&event).unwrap();
        expected.push(b'\n');
        let budget = expected.len() as u64;
        assert_eq!(encode(&event, budget).unwrap(), expected);
        assert_eq!(encode(&event, budget - 1), Err(LogError::Capacity));
        assert_eq!(encode(&event, 0), Err(LogError::Capacity));
        assert_eq!(encode(&event, 1), Err(LogError::Capacity));
    }

    #[test]
    fn counter_rejects_before_accepting_oversized_chunk() {
        let mut counter = Counter {
            remaining: 2,
            bytes: 0,
            exceeded: false,
        };
        assert!(counter.write_all(b"abc").is_err());
        assert_eq!(counter.bytes, 0);
        assert_eq!(counter.remaining, 2);
        assert!(counter.exceeded);
    }

    #[test]
    fn rejected_jsonl_does_not_rotate_or_modify_existing_file() {
        use crate::{FileSink, FileSinkConfig, LogSink};
        let path = std::env::temp_dir().join(format!(
            "appcore-log-sized-rejection-{}.jsonl",
            std::process::id()
        ));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        file.write_all(b"preserved\n").unwrap();
        drop(file);
        let sink = FileSink::new(FileSinkConfig {
            path: path.clone(),
            max_bytes: 10,
            sync_each_write: false,
            retention: 1,
            archive: None,
        })
        .unwrap();
        let event = LogEvent::new(0, Severity::Info, Verbosity::V4, "test", "too large");
        assert_eq!(sink.emit(&event), Err(LogError::Capacity));
        assert_eq!(std::fs::read(&path).unwrap(), b"preserved\n");
        assert!(!path.with_extension("jsonl.1").exists());
        std::fs::remove_file(path).unwrap();
    }
}
