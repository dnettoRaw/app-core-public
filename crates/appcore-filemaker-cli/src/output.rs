// =============================================================================
//        #######
//     ###       ###     F: output.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/02 21:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/02 21:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Owns bounded stdout serialization without a complete intermediate string.

use std::io::{self, Write};

use serde::Serialize;

use crate::failure::{software, CliFailure, CliResult, EXIT_IO, EXIT_TEMPFAIL};

pub(crate) const MAX_CLI_OUTPUT_BYTES: usize = 512 * 1024 * 1024;

pub(crate) enum CliOutput {
    Text(String),
    Json(Box<dyn JsonPayload>),
}

impl CliOutput {
    pub(crate) fn text(value: String) -> Self {
        Self::Text(value)
    }

    pub(crate) fn response<T>(value: T, human: String, json: bool) -> Self
    where
        T: Serialize + 'static,
    {
        if json {
            Self::Json(Box::new(OwnedJson(value)))
        } else {
            Self::Text(human)
        }
    }

    pub(crate) fn write_to(&self, writer: &mut dyn Write) -> CliResult<()> {
        match self {
            Self::Text(value) => write_text(writer, value),
            Self::Json(value) => value.write_pretty(writer),
        }?;
        writer
            .flush()
            .map_err(|error| output_io(error, matches!(self, Self::Json(_))))
    }
}

pub(crate) trait JsonPayload {
    fn write_pretty(&self, writer: &mut dyn Write) -> CliResult<()>;
}

struct OwnedJson<T>(T);

impl<T: Serialize> JsonPayload for OwnedJson<T> {
    fn write_pretty(&self, writer: &mut dyn Write) -> CliResult<()> {
        write_json_with_limit(writer, &self.0, MAX_CLI_OUTPUT_BYTES)
    }
}

fn write_text(writer: &mut dyn Write, value: &str) -> CliResult<()> {
    let bytes = value
        .len()
        .checked_add(1)
        .ok_or_else(|| output_limit(false))?;
    if bytes > MAX_CLI_OUTPUT_BYTES {
        return Err(output_limit(false));
    }
    writer
        .write_all(value.as_bytes())
        .and_then(|()| writer.write_all(b"\n"))
        .map_err(|error| output_io(error, false))
}

fn write_json_with_limit<T: Serialize>(
    writer: &mut dyn Write,
    value: &T,
    max_bytes: usize,
) -> CliResult<()> {
    let payload_limit = max_bytes.checked_sub(1).ok_or_else(|| output_limit(true))?;
    let mut counter = JsonCounter::new(payload_limit);
    if let Err(error) = serde_json::to_writer_pretty(&mut counter, value) {
        if counter.exceeded {
            return Err(output_limit(true));
        }
        return Err(json_failure(error));
    }
    let (remaining, exceeded) = {
        let mut output = ExactJsonWriter::new(writer, counter.written);
        if let Err(error) = serde_json::to_writer_pretty(&mut output, value) {
            if output.exceeded {
                return Err(software(
                    "CLI JSON serialization changed after bounded sizing",
                    true,
                ));
            }
            return Err(json_failure(error));
        }
        (output.remaining, output.exceeded)
    };
    if remaining != 0 || exceeded {
        return Err(software(
            "CLI JSON serialization changed after bounded sizing",
            true,
        ));
    }
    writer
        .write_all(b"\n")
        .map_err(|error| output_io(error, true))
}

struct JsonCounter {
    written: usize,
    limit: usize,
    exceeded: bool,
}

impl JsonCounter {
    const fn new(limit: usize) -> Self {
        Self {
            written: 0,
            limit,
            exceeded: false,
        }
    }
}

impl Write for JsonCounter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(written) = self.written.checked_add(bytes.len()) else {
            self.exceeded = true;
            return Err(io::Error::other("CLI JSON byte count overflow"));
        };
        if written > self.limit {
            self.exceeded = true;
            return Err(io::Error::other("CLI JSON exceeds stdout limit"));
        }
        self.written = written;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct ExactJsonWriter<'a> {
    writer: &'a mut dyn Write,
    remaining: usize,
    exceeded: bool,
}

impl<'a> ExactJsonWriter<'a> {
    const fn new(writer: &'a mut dyn Write, expected: usize) -> Self {
        Self {
            writer,
            remaining: expected,
            exceeded: false,
        }
    }
}

impl Write for ExactJsonWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.remaining {
            self.exceeded = true;
            return Err(io::Error::other("CLI JSON exceeded its measured size"));
        }
        self.writer.write_all(bytes)?;
        self.remaining -= bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

fn json_failure(error: serde_json::Error) -> CliFailure {
    if error.is_io() {
        CliFailure::io(EXIT_IO, "FM-CLI-IO", error.to_string(), true)
    } else {
        software(error.to_string(), true)
    }
}

fn output_io(error: io::Error, json: bool) -> CliFailure {
    CliFailure::io(EXIT_IO, "FM-CLI-IO", error.to_string(), json)
}

fn output_limit(json: bool) -> CliFailure {
    CliFailure::new(
        EXIT_TEMPFAIL,
        "FM-CLI-LIMIT",
        "CLI output exceeds the 512 MiB byte limit",
        json,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn direct_json_output_matches_the_pretty_contract() {
        let value = json!({"alpha": [1, 2, 3], "unicode": "日本語 العربية"});
        let expected = format!("{}\n", serde_json::to_string_pretty(&value).unwrap());
        let mut output = Vec::new();
        assert!(write_json_with_limit(&mut output, &value, 1_024).is_ok());
        assert_eq!(output, expected.as_bytes());
    }

    #[test]
    fn limit_failure_happens_before_writing() {
        let mut output = Vec::new();
        let error = write_json_with_limit(&mut output, &"oversized", 4).unwrap_err();
        assert_eq!(error.exit_code(), EXIT_TEMPFAIL);
        assert!(output.is_empty());
    }
}
