// =============================================================================
//        #######
//     ###       ###     F: artifact_result.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/07 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/07 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Stream exporter bytes into bounded base64 without retaining the raw artifact.

use crate::{BridgeError, BridgeResult};
use base64::Engine as _;
use serde::Serialize;
use serde_json::Value;
use std::io::{self, Write};

const RAW_CHUNK_BYTES: usize = 6 * 1_024;
const ENCODED_CHUNK_BYTES: usize = RAW_CHUNK_BYTES / 3 * 4;

#[derive(Serialize)]
struct Envelope<'a, T> {
    media_type: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    table: Option<&'a str>,
    bytes: usize,
    base64: &'a str,
    loss_report: &'a T,
}

pub(crate) struct ArtifactWriter {
    encoded: String,
    pending: [u8; 3],
    pending_len: usize,
    raw_bytes: usize,
    max_encoded_bytes: usize,
}

impl ArtifactWriter {
    pub(crate) fn new(max_encoded_bytes: usize) -> Self {
        Self {
            encoded: String::new(),
            pending: [0; 3],
            pending_len: 0,
            raw_bytes: 0,
            max_encoded_bytes,
        }
    }

    fn append_triplets(&mut self, mut bytes: &[u8]) -> io::Result<()> {
        let mut scratch = [0_u8; ENCODED_CHUNK_BYTES];
        while bytes.len() >= 3 {
            let raw_len = bytes.len().min(RAW_CHUNK_BYTES);
            let raw_len = raw_len - raw_len % 3;
            let encoded_len = raw_len / 3 * 4;
            self.reserve_encoded(encoded_len)?;
            let written = base64::engine::general_purpose::STANDARD
                .encode_slice(&bytes[..raw_len], &mut scratch[..encoded_len])
                .map_err(|_| io::Error::other("base64 scratch is too small"))?;
            let encoded = std::str::from_utf8(&scratch[..written])
                .map_err(|_| io::Error::other("base64 encoder emitted invalid UTF-8"))?;
            self.encoded.push_str(encoded);
            bytes = &bytes[raw_len..];
        }
        if !bytes.is_empty() {
            self.pending[..bytes.len()].copy_from_slice(bytes);
            self.pending_len = bytes.len();
        }
        Ok(())
    }

    fn reserve_encoded(&mut self, additional: usize) -> io::Result<()> {
        let required = self
            .encoded
            .len()
            .checked_add(additional)
            .ok_or_else(|| io::Error::other("base64 size overflow"))?;
        if required > self.max_encoded_bytes {
            return Err(io::Error::other("artifact exceeds result budget"));
        }
        self.encoded
            .try_reserve_exact(additional)
            .map_err(|_| io::Error::other("base64 allocation failed"))?;
        Ok(())
    }

    fn finish(mut self) -> BridgeResult<EncodedArtifact> {
        if self.pending_len != 0 {
            self.reserve_encoded(4)
                .map_err(|error| BridgeError::Policy(error.to_string()))?;
            let mut encoded = [0_u8; 4];
            let written = base64::engine::general_purpose::STANDARD
                .encode_slice(&self.pending[..self.pending_len], &mut encoded)
                .map_err(|_| BridgeError::Policy("base64 scratch is too small".to_owned()))?;
            let encoded = std::str::from_utf8(&encoded[..written]).map_err(|_| {
                BridgeError::Policy("base64 encoder emitted invalid UTF-8".to_owned())
            })?;
            self.encoded.push_str(encoded);
        }
        Ok(EncodedArtifact {
            encoded: self.encoded,
            raw_bytes: self.raw_bytes,
        })
    }
}

impl Write for ArtifactWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.raw_bytes = self
            .raw_bytes
            .checked_add(bytes.len())
            .ok_or_else(|| io::Error::other("artifact byte count overflow"))?;
        let mut remaining = bytes;
        if self.pending_len != 0 {
            let copied = remaining.len().min(3 - self.pending_len);
            self.pending[self.pending_len..self.pending_len + copied]
                .copy_from_slice(&remaining[..copied]);
            self.pending_len += copied;
            remaining = &remaining[copied..];
            if self.pending_len == 3 {
                let pending = self.pending;
                self.pending_len = 0;
                self.append_triplets(&pending)?;
            }
        }
        self.append_triplets(remaining)?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct EncodedArtifact {
    encoded: String,
    raw_bytes: usize,
}

pub(crate) fn finish<T: Serialize>(
    media_type: &str,
    table: Option<&str>,
    writer: ArtifactWriter,
    loss_report: &T,
    limit: usize,
) -> BridgeResult<Value> {
    let artifact = writer.finish()?;
    let overhead_limit = limit
        .checked_sub(artifact.encoded.len())
        .ok_or_else(|| BridgeError::Policy("artifact exceeds result budget".to_owned()))?;
    let envelope = Envelope {
        media_type,
        table,
        bytes: artifact.raw_bytes,
        base64: "",
        loss_report,
    };
    crate::session::enforce_result_limit(&envelope, overhead_limit)?;
    let mut value = serde_json::to_value(envelope).map_err(crate::error::json_error)?;
    // Base64 alphabet needs no JSON escaping; empty-string quotes were counted above.
    value["base64"] = Value::String(artifact.encoded);
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded(bytes: &[u8], split: usize, limit: usize) -> BridgeResult<Value> {
        let mut writer = ArtifactWriter::new(limit);
        for chunk in bytes.chunks(split) {
            writer.write_all(chunk).unwrap();
        }
        finish(
            "text/plain",
            Some("table-é"),
            writer,
            &serde_json::json!({"message": "é日\""}),
            limit,
        )
    }

    #[test]
    fn streaming_base64_matches_every_input_boundary() {
        for length in 0..=32 {
            let bytes: Vec<u8> = (0..length).map(|value| value as u8).collect();
            let expected = base64::engine::general_purpose::STANDARD.encode(&bytes);
            for split in 1..=7 {
                assert_eq!(encoded(&bytes, split, 1_024).unwrap()["base64"], expected);
            }
        }
    }

    #[test]
    fn full_envelope_budget_includes_metadata_padding_and_unicode() {
        for bytes in [&b""[..], &b"a"[..], &b"ab"[..], &b"abc"[..]] {
            let value = encoded(bytes, 1, 1_024).unwrap();
            let exact = serde_json::to_vec(&value).unwrap().len();
            assert_eq!(encoded(bytes, 1, exact).unwrap(), value);
            assert!(encoded(bytes, 1, exact - 1).is_err());
        }
    }

    #[test]
    fn writer_rejects_before_growing_beyond_encoded_budget() {
        let mut writer = ArtifactWriter::new(4);
        writer.write_all(b"abc").unwrap();
        assert_eq!(writer.encoded.len(), 4);
        assert!(writer.write_all(b"def").is_err());
        assert_eq!(writer.encoded.len(), 4);
    }
}
