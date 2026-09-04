// =============================================================================
//        #######
//     ###       ###     F: pdf_stream.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/31 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/31 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Streams independent PDF objects and writes a bounded classic cross-reference table.

use std::fmt::Write as _;
use std::io::{self, Write};

use pdf_writer::{Chunk, Ref};

use crate::{ErrorCode, FileMakerError, Result};

const MISSING_OFFSET: usize = usize::MAX;

pub(crate) struct PdfDocument<'a> {
    writer: &'a mut dyn Write,
    limit: usize,
    written: usize,
    offsets: Vec<usize>,
    catalog: Ref,
    info: Ref,
}

impl<'a> PdfDocument<'a> {
    pub(crate) fn new(
        writer: &'a mut dyn Write,
        limit: usize,
        catalog: Ref,
        info: Ref,
    ) -> Result<Self> {
        let mut document = Self {
            writer,
            limit,
            written: 0,
            offsets: vec![MISSING_OFFSET],
            catalog,
            info,
        };
        document.write(b"%PDF-1.7\n%\x80\x80\x80\x80\n\n")?;
        Ok(document)
    }

    pub(crate) fn object(
        &mut self,
        reference: Ref,
        build: impl FnOnce(&mut Chunk) -> Result<()>,
    ) -> Result<()> {
        let mut chunk = Chunk::new();
        build(&mut chunk)?;
        let mut refs = chunk.refs();
        if refs.next() != Some(reference) || refs.next().is_some() {
            return Err(pdf_error(
                "PDF chunk must contain exactly its declared object",
            ));
        }
        self.register_object(reference)?;
        self.write(chunk.as_bytes())
    }

    pub(crate) fn stream_object(
        &mut self,
        reference: Ref,
        content_length: usize,
        build: impl FnOnce(&mut dyn Write) -> Result<()>,
    ) -> Result<()> {
        let length = i32::try_from(content_length)
            .map_err(|_| limit_error("PDF stream exceeds the format length range"))?;
        let header = format!(
            "{} 0 obj\n<<\n  /Length {length}\n>>\nstream\n",
            reference.get()
        );
        const FOOTER: &[u8] = b"\nendstream\nendobj\n\n";
        let object_length = header
            .len()
            .checked_add(content_length)
            .and_then(|value| value.checked_add(FOOTER.len()))
            .ok_or_else(|| limit_error("PDF stream byte accounting overflow"))?;
        self.ensure_capacity(object_length)?;
        self.register_object(reference)?;
        self.write(header.as_bytes())?;
        let mut writer = ExactWriter::new(self.writer, content_length);
        build(&mut writer)?;
        if writer.remaining != 0 {
            return Err(pdf_error("PDF stream wrote fewer bytes than declared"));
        }
        self.written = self
            .written
            .checked_add(content_length)
            .ok_or_else(|| limit_error("PDF byte accounting overflow"))?;
        self.write(FOOTER)
    }

    pub(crate) fn finish(mut self) -> Result<usize> {
        let xref_offset = self.written;
        self.write(format!("xref\n0 {}\n", self.offsets.len()).as_bytes())?;
        self.write(b"0000000000 65535 f \n")?;
        let mut entry = String::with_capacity(21);
        for index in 1..self.offsets.len() {
            let offset = self.offsets[index];
            if offset == MISSING_OFFSET {
                self.write(b"0000000000 00000 f \n")?;
            } else if offset <= 9_999_999_999 {
                entry.clear();
                writeln!(entry, "{offset:010} 00000 n ")
                    .map_err(|_| pdf_error("cannot format PDF xref entry"))?;
                self.write(entry.as_bytes())?;
            } else {
                return Err(limit_error("PDF offset exceeds classic xref range"));
            }
        }
        self.write(
            format!(
                "trailer\n<<\n  /Size {}\n  /Root {} 0 R\n  /Info {} 0 R\n>>\nstartxref\n{xref_offset}\n%%EOF",
                self.offsets.len(),
                self.catalog.get(),
                self.info.get(),
            )
            .as_bytes(),
        )?;
        Ok(self.written)
    }

    fn write(&mut self, bytes: &[u8]) -> Result<()> {
        let next = self.ensure_capacity(bytes.len())?;
        self.writer
            .write_all(bytes)
            .map_err(|error| FileMakerError::new(ErrorCode::ExportWrite, error.to_string()))?;
        self.written = next;
        Ok(())
    }

    fn ensure_capacity(&self, additional: usize) -> Result<usize> {
        let next = self
            .written
            .checked_add(additional)
            .ok_or_else(|| limit_error("PDF byte accounting overflow"))?;
        if next > self.limit {
            return Err(limit_error("PDF exceeds configured output limit"));
        }
        Ok(next)
    }

    fn register_object(&mut self, reference: Ref) -> Result<()> {
        let index = usize::try_from(reference.get())
            .map_err(|_| limit_error("PDF reference index exceeds usize"))?;
        if self.offsets.len() <= index {
            self.offsets.resize(index + 1, MISSING_OFFSET);
        }
        if self.offsets[index] != MISSING_OFFSET {
            return Err(pdf_error("PDF object reference was written more than once"));
        }
        self.offsets[index] = self.written;
        Ok(())
    }
}

struct ExactWriter<'a> {
    writer: &'a mut dyn Write,
    remaining: usize,
}

impl<'a> ExactWriter<'a> {
    const fn new(writer: &'a mut dyn Write, remaining: usize) -> Self {
        Self { writer, remaining }
    }
}

impl Write for ExactWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.remaining {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "PDF stream exceeds its declared length",
            ));
        }
        let written = self.writer.write(bytes)?;
        self.remaining = self
            .remaining
            .checked_sub(written)
            .ok_or_else(|| io::Error::other("PDF writer exceeded requested byte count"))?;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

fn pdf_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::ExportWrite, message)
}

fn limit_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::LimitExceeded, message)
}
