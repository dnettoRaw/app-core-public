// =============================================================================
//        #######
//     ###       ###     F: csv.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Streams bounded RFC-4180-style records without duplicating textual cells.

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::io::Write;

use crate::{
    Dataset, ErrorCode, ExportCapabilities, ExportLossReport, ExportOutcome, FileMakerError,
    ResourceLimits, Result, TableSpec,
};

/// Streams a dataset as RFC-4180-style UTF-8 CSV in table column order.
pub fn export_dataset_csv(
    spec: &TableSpec,
    dataset: &dyn Dataset,
    limits: &ResourceLimits,
    writer: &mut dyn Write,
) -> Result<ExportOutcome> {
    limits.validate()?;
    spec.validate()?;
    let mut bounded = BoundedWriter::new(writer, limits.max_output_bytes);
    write_record(
        &mut bounded,
        spec.columns.iter().map(|column| column.header.as_str()),
    )?;
    spec.visit_bounded(dataset, &mut |_, row| {
        write_record(
            &mut bounded,
            spec.columns.iter().map(|column| {
                row.get(&column.field)
                    .map_or(Cow::Borrowed(""), display_value)
            }),
        )
    })?;
    Ok(ExportOutcome {
        bytes_written: bounded.written,
        loss_report: ExportLossReport::default(),
        capabilities: BTreeSet::from([ExportCapabilities::Semantic]),
    })
}

/// Streams a dataset into a bounded in-memory CSV byte vector.
pub fn export_dataset_csv_bytes(
    spec: &TableSpec,
    dataset: &dyn Dataset,
    limits: &ResourceLimits,
) -> Result<(Vec<u8>, ExportOutcome)> {
    let mut bytes = Vec::new();
    let outcome = export_dataset_csv(spec, dataset, limits, &mut bytes)?;
    Ok((bytes, outcome))
}

fn write_record<T, S>(writer: &mut BoundedWriter<'_>, values: T) -> Result<()>
where
    T: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut first = true;
    for value in values {
        if !first {
            writer.write_all(b",")?;
        }
        first = false;
        let value = value.as_ref();
        write_field(writer, value)?;
    }
    writer.write_all(b"\r\n")
}

fn write_field(writer: &mut BoundedWriter<'_>, value: &str) -> Result<()> {
    if !value.contains([',', '"', '\r', '\n']) {
        return writer.write_all(value.as_bytes());
    }
    writer.write_all(b"\"")?;
    let bytes = value.as_bytes();
    let mut start = 0;
    for (index, _) in value.match_indices('"') {
        writer.write_all(&bytes[start..index])?;
        writer.write_all(b"\"\"")?;
        start = index + 1;
    }
    writer.write_all(&bytes[start..])?;
    writer.write_all(b"\"")
}

fn display_value(value: &crate::DataValue) -> Cow<'_, str> {
    match value {
        crate::DataValue::String(value)
        | crate::DataValue::Date(value)
        | crate::DataValue::DateTime(value) => Cow::Borrowed(value),
        crate::DataValue::Array(_) => Cow::Borrowed("[array]"),
        crate::DataValue::Object(_) => Cow::Borrowed("[object]"),
        crate::DataValue::Null => Cow::Borrowed(""),
        _ => Cow::Owned(value.display()),
    }
}

struct BoundedWriter<'a> {
    inner: &'a mut dyn Write,
    limit: usize,
    written: usize,
}

impl<'a> BoundedWriter<'a> {
    const fn new(inner: &'a mut dyn Write, limit: usize) -> Self {
        Self {
            inner,
            limit,
            written: 0,
        }
    }

    fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
        let next = self
            .written
            .checked_add(bytes.len())
            .ok_or_else(|| limit_error("CSV byte accounting overflow"))?;
        if next > self.limit {
            return Err(limit_error("CSV exceeds configured output limit"));
        }
        self.inner
            .write_all(bytes)
            .map_err(|error| FileMakerError::new(ErrorCode::ExportWrite, error.to_string()))?;
        self.written = next;
        Ok(())
    }
}

fn limit_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::LimitExceeded, message)
}
