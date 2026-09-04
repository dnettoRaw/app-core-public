// =============================================================================
//        #######
//     ###       ###     F: fingerprint.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Defines bounded fingerprint contracts and behavior for this crate.

use std::collections::BTreeSet;
use std::io::{self, Write};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    AssetResolver, DataValue, ElementIr, ErrorCode, FileMakerError, FontManager, Patch,
    PatchOperation, ResourceLimits, Result, TemplateIr, ENGINE_VERSION, FILEMAKER_SCHEMA_V1,
};

const DEFAULT_MAX_FINGERPRINT_BYTES: usize = 512 * 1024 * 1024;

/// SHA-256 identity of every input affecting a bound document.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DocumentFingerprint([u8; 32]);

impl DocumentFingerprint {
    /// Computes a fingerprint from canonical IR/data plus explicit assets and fonts.
    pub fn compute(
        template: &TemplateIr,
        data: &DataValue,
        assets: Option<&dyn AssetResolver>,
        fonts: &FontManager,
        limits: &ResourceLimits,
    ) -> Result<Self> {
        Self::compute_with_patches(template, data, &[], assets, fonts, limits)
    }

    /// Computes the cache identity for the complete bind input, including patches.
    pub fn compute_with_patches(
        template: &TemplateIr,
        data: &DataValue,
        patches: &[Patch],
        assets: Option<&dyn AssetResolver>,
        fonts: &FontManager,
        limits: &ResourceLimits,
    ) -> Result<Self> {
        let mut builder = FingerprintBuilder::with_max_bytes(limits.max_output_bytes)?;
        builder.field("schema", FILEMAKER_SCHEMA_V1.as_bytes())?;
        builder.field("engine", ENGINE_VERSION.as_bytes())?;
        builder.serialized("template", template)?;
        builder.serialized("data", data)?;
        builder.serialized("patches", &patches)?;
        for (name, digest) in fonts.digests() {
            builder.field("font-name", name.as_bytes())?;
            builder.field("font-digest", digest)?;
        }
        for name in fonts.fallback_names() {
            builder.field("font-fallback", name.as_bytes())?;
        }
        let mut names = asset_names(&template.elements);
        for patch in patches {
            for operation in &patch.operations {
                if let PatchOperation::Add { element, .. }
                | PatchOperation::Replace { element, .. } = operation
                {
                    names.extend(asset_names(std::slice::from_ref(element)));
                }
            }
        }
        if !names.is_empty() && assets.is_none() {
            return Err(fingerprint_error(
                "fingerprint requires a resolver for referenced assets",
            ));
        }
        if let Some(resolver) = assets {
            for name in names {
                let asset = resolver.resolve_asset(name, limits.max_asset_bytes)?;
                builder.field("asset-name", name.as_bytes())?;
                builder.field("asset-media-type", asset.media_type.as_bytes())?;
                builder.field("asset-digest", &asset.digest)?;
            }
        }
        Ok(builder.finish())
    }

    /// Returns lowercase hexadecimal without allocating intermediary data.
    #[must_use]
    pub fn to_hex(self) -> String {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let mut result = String::with_capacity(64);
        for byte in self.0 {
            result.push(char::from(DIGITS[usize::from(byte >> 4)]));
            result.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
        }
        result
    }

    /// Returns the raw digest.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Length-delimited deterministic SHA-256 input builder.
pub struct FingerprintBuilder {
    hasher: Sha256,
    remaining: usize,
}

impl FingerprintBuilder {
    /// Starts a fingerprint with a domain-separation marker and 512 MiB field budget.
    #[must_use]
    pub fn new() -> Self {
        Self::with_max_bytes_unchecked(DEFAULT_MAX_FINGERPRINT_BYTES)
    }

    /// Starts a fingerprint with an aggregate budget for all framed fields.
    pub fn with_max_bytes(max_bytes: usize) -> Result<Self> {
        if max_bytes == 0 {
            return Err(fingerprint_limit_error(
                "fingerprint aggregate byte limit must be non-zero",
            ));
        }
        Ok(Self::with_max_bytes_unchecked(max_bytes))
    }

    fn with_max_bytes_unchecked(max_bytes: usize) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"appcore-filemaker-fingerprint-v1\0");
        Self {
            hasher,
            remaining: max_bytes,
        }
    }

    /// Adds one named byte field with unambiguous length boundaries.
    pub fn field(&mut self, name: &str, bytes: &[u8]) -> Result<()> {
        let framed_bytes = framed_field_size(name, bytes.len())?;
        self.require_remaining(framed_bytes)?;
        let byte_len = u64::try_from(bytes.len())
            .map_err(|_| fingerprint_error("fingerprint field is too long"))?;
        frame_field(&mut self.hasher, name, byte_len)?;
        self.hasher.update(bytes);
        self.remaining -= framed_bytes;
        Ok(())
    }

    /// Serializes canonical field-order JSON directly into the digest.
    ///
    /// Serialization runs once for bounded sizing and once for hashing so the
    /// length prefix can precede the payload without retaining a JSON buffer.
    /// Custom `Serialize` implementations must produce the same bytes in both
    /// passes; a size change fails without mutating the builder.
    pub fn serialized<T: Serialize>(&mut self, name: &str, value: &T) -> Result<()> {
        let framing_bytes = framed_field_size(name, 0)?;
        self.require_remaining(framing_bytes)?;
        let mut counter = JsonLengthWriter::new(self.remaining - framing_bytes);
        if let Err(error) = serde_json::to_writer(&mut counter, value) {
            if counter.exceeded {
                return Err(fingerprint_limit_error(
                    "serialized fingerprint fields exceed the aggregate byte limit",
                ));
            }
            return Err(serialization_error(error));
        }
        let framed_bytes = framing_bytes
            .checked_add(counter.written)
            .ok_or_else(|| fingerprint_limit_error("fingerprint field size overflow"))?;
        let byte_len = u64::try_from(counter.written)
            .map_err(|_| fingerprint_error("fingerprint field is too long"))?;
        let mut candidate = self.hasher.clone();
        frame_field(&mut candidate, name, byte_len)?;
        let (remaining, exceeded) = {
            let mut writer = JsonDigestWriter::new(&mut candidate, counter.written);
            let result = serde_json::to_writer(&mut writer, value);
            if let Err(error) = result {
                if writer.exceeded {
                    return Err(fingerprint_error(
                        "fingerprint serialization changed while hashing",
                    ));
                }
                return Err(serialization_error(error));
            }
            (writer.remaining, writer.exceeded)
        };
        if remaining != 0 || exceeded {
            return Err(fingerprint_error(
                "fingerprint serialization changed while hashing",
            ));
        }
        self.hasher = candidate;
        self.remaining -= framed_bytes;
        Ok(())
    }

    fn require_remaining(&self, bytes: usize) -> Result<()> {
        if bytes > self.remaining {
            return Err(fingerprint_limit_error(
                "fingerprint fields exceed the aggregate byte limit",
            ));
        }
        Ok(())
    }

    /// Finishes the digest.
    #[must_use]
    pub fn finish(self) -> DocumentFingerprint {
        DocumentFingerprint(self.hasher.finalize().into())
    }
}

struct JsonLengthWriter {
    written: usize,
    limit: usize,
    exceeded: bool,
}

impl JsonLengthWriter {
    const fn new(limit: usize) -> Self {
        Self {
            written: 0,
            limit,
            exceeded: false,
        }
    }
}

impl Write for JsonLengthWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(written) = self.written.checked_add(bytes.len()) else {
            self.exceeded = true;
            return Err(io::Error::other("fingerprint JSON length overflow"));
        };
        if written > self.limit {
            self.exceeded = true;
            return Err(io::Error::other("fingerprint JSON exceeded byte limit"));
        }
        self.written = written;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct JsonDigestWriter<'a> {
    hasher: &'a mut Sha256,
    remaining: usize,
    exceeded: bool,
}

impl<'a> JsonDigestWriter<'a> {
    const fn new(hasher: &'a mut Sha256, expected: usize) -> Self {
        Self {
            hasher,
            remaining: expected,
            exceeded: false,
        }
    }
}

impl Write for JsonDigestWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.remaining {
            self.exceeded = true;
            return Err(io::Error::other("fingerprint JSON exceeded measured size"));
        }
        self.hasher.update(bytes);
        self.remaining -= bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn frame_field(hasher: &mut Sha256, name: &str, byte_len: u64) -> Result<()> {
    let name_len = u64::try_from(name.len())
        .map_err(|_| fingerprint_error("fingerprint field name is too long"))?;
    hasher.update(name_len.to_be_bytes());
    hasher.update(name.as_bytes());
    hasher.update(byte_len.to_be_bytes());
    Ok(())
}

fn framed_field_size(name: &str, payload_bytes: usize) -> Result<usize> {
    16_usize
        .checked_add(name.len())
        .and_then(|size| size.checked_add(payload_bytes))
        .ok_or_else(|| fingerprint_limit_error("fingerprint field size overflow"))
}

fn serialization_error(error: serde_json::Error) -> FileMakerError {
    fingerprint_error(format!("cannot serialize fingerprint field: {error}"))
}

impl Default for FingerprintBuilder {
    fn default() -> Self {
        Self::new()
    }
}

fn asset_names(elements: &[ElementIr]) -> BTreeSet<&str> {
    let mut names = BTreeSet::new();
    let mut stack = elements.iter().collect::<Vec<_>>();
    while let Some(element) = stack.pop() {
        if let Some(asset) = &element.asset {
            names.insert(asset.as_str());
        }
        stack.extend(element.children.iter());
    }
    names
}

fn fingerprint_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::Validation, message)
}

fn fingerprint_limit_error(message: impl Into<String>) -> FileMakerError {
    FileMakerError::new(ErrorCode::LimitExceeded, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct ChangingSerialization(Cell<bool>);

    impl Serialize for ChangingSerialization {
        fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            if self.0.replace(true) {
                "longer".serialize(serializer)
            } else {
                "x".serialize(serializer)
            }
        }
    }

    #[test]
    fn length_boundaries_prevent_ambiguous_hashes() {
        let mut left = FingerprintBuilder::new();
        left.field("a", b"bc").unwrap();
        let mut right = FingerprintBuilder::new();
        right.field("ab", b"c").unwrap();
        assert_ne!(left.finish(), right.finish());
    }

    #[test]
    fn streaming_serialization_preserves_the_buffered_v1_fingerprint() {
        let value = serde_json::json!({
            "alpha": [1, 2, 3],
            "nested": {"stable": true},
            "unicode": "日本語 العربية",
        });
        let mut streamed = FingerprintBuilder::new();
        streamed.serialized("value", &value).unwrap();

        let mut buffered = FingerprintBuilder::new();
        buffered
            .field("value", &serde_json::to_vec(&value).unwrap())
            .unwrap();
        assert_eq!(streamed.finish(), buffered.finish());
    }

    #[test]
    fn aggregate_budget_counts_field_framing_and_serialized_bytes() {
        let mut exact_field = FingerprintBuilder::with_max_bytes(18).unwrap();
        exact_field.field("a", b"b").unwrap();
        let mut short_field = FingerprintBuilder::with_max_bytes(17).unwrap();
        assert!(short_field.field("a", b"b").is_err());

        let mut exact_json = FingerprintBuilder::with_max_bytes(20).unwrap();
        exact_json.serialized("v", &"x").unwrap();
        let mut short_json = FingerprintBuilder::with_max_bytes(19).unwrap();
        assert!(short_json.serialized("v", &"x").is_err());
    }

    #[test]
    fn changing_serialization_does_not_mutate_the_builder() {
        let mut actual = FingerprintBuilder::with_max_bytes(128).unwrap();
        actual.field("before", b"stable").unwrap();
        assert!(actual
            .serialized("changing", &ChangingSerialization(Cell::new(false)))
            .is_err());
        actual.field("after", b"stable").unwrap();

        let mut expected = FingerprintBuilder::with_max_bytes(128).unwrap();
        expected.field("before", b"stable").unwrap();
        expected.field("after", b"stable").unwrap();
        assert_eq!(actual.finish(), expected.finish());
    }
}
