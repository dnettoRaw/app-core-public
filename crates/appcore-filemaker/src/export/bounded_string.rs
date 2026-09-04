// =============================================================================
//        #######
//     ###       ###     F: bounded_string.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Bounded buffered, counting, and caller-owned streaming formatting sinks.

use std::fmt;
use std::io::Write;

use crate::{ErrorCode, FileMakerError, Result};

pub(crate) trait FormattedOutput: fmt::Write {
    fn push_str(&mut self, value: &str) -> Result<()>;
    fn write_bytes(&mut self, value: &[u8]) -> Result<()>;
}

pub(crate) struct StreamingOutput<'a> {
    writer: &'a mut dyn Write,
    limit: usize,
    written: usize,
    failure: Option<FileMakerError>,
}

pub(crate) struct CountingOutput {
    limit: usize,
    written: usize,
    failure: Option<FileMakerError>,
}

impl<'a> StreamingOutput<'a> {
    pub(crate) fn new(writer: &'a mut dyn Write, limit: usize) -> Self {
        Self {
            writer,
            limit,
            written: 0,
            failure: None,
        }
    }

    pub(crate) fn finish(self) -> Result<usize> {
        match self.failure {
            Some(error) => Err(error),
            None => Ok(self.written),
        }
    }

    pub(crate) fn write_bytes(&mut self, value: &[u8]) {
        if self.failure.is_some() {
            return;
        }
        let Some(length) = self.written.checked_add(value.len()) else {
            self.failure = Some(output_limit_error());
            return;
        };
        if length > self.limit {
            self.failure = Some(output_limit_error());
            return;
        }
        if let Err(error) = self.writer.write_all(value) {
            self.failure = Some(FileMakerError::new(
                ErrorCode::ExportWrite,
                error.to_string(),
            ));
            return;
        }
        self.written = length;
    }
}

impl CountingOutput {
    pub(crate) const fn new(limit: usize) -> Self {
        Self {
            limit,
            written: 0,
            failure: None,
        }
    }

    pub(crate) fn finish(self) -> Result<usize> {
        match self.failure {
            Some(error) => Err(error),
            None => Ok(self.written),
        }
    }

    fn count(&mut self, length: usize) -> Result<()> {
        if self.failure.is_some() {
            return Err(output_limit_error());
        }
        let Some(written) = self.written.checked_add(length) else {
            self.failure = Some(output_limit_error());
            return Err(output_limit_error());
        };
        if written > self.limit {
            self.failure = Some(output_limit_error());
            return Err(output_limit_error());
        }
        self.written = written;
        Ok(())
    }
}

impl fmt::Write for StreamingOutput<'_> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.write_bytes(value.as_bytes());
        Ok(())
    }
}

impl FormattedOutput for StreamingOutput<'_> {
    fn push_str(&mut self, value: &str) -> Result<()> {
        self.write_bytes(value.as_bytes());
        Ok(())
    }

    fn write_bytes(&mut self, value: &[u8]) -> Result<()> {
        StreamingOutput::write_bytes(self, value);
        Ok(())
    }
}

impl fmt::Write for CountingOutput {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.count(value.len()).map_err(|_| fmt::Error)
    }
}

impl FormattedOutput for CountingOutput {
    fn push_str(&mut self, value: &str) -> Result<()> {
        self.count(value.len())
    }

    fn write_bytes(&mut self, value: &[u8]) -> Result<()> {
        self.count(value.len())
    }
}

pub(crate) fn output_limit_error() -> FileMakerError {
    FileMakerError::new(
        ErrorCode::LimitExceeded,
        "formatted export exceeds configured output limit",
    )
}
