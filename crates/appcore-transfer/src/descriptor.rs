// =============================================================================
//        #######
//     ###       ###     F: descriptor.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/06 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/06 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Versioned immutable object identity and hard resource bounds.
use crate::{TransferError as E, TransferResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
/// Admission bounds, validated before network or persistent work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferLimits {
    /// Maximum complete object bytes, at most 256 MiB.
    pub max_object_bytes: u64,
    /// Maximum block bytes, at most 1 MiB.
    pub max_block_bytes: u32,
    /// Maximum hashes/blocks per object, at most 4096.
    pub max_blocks: usize,
    /// Maximum store files, including metadata and lock, at most 65536.
    pub max_files: usize,
    /// Store bytes including complete objects, partial blocks, staging and metadata.
    pub max_store_bytes: u64,
}
impl Default for TransferLimits {
    fn default() -> Self {
        Self {
            max_object_bytes: 64 * 1024 * 1024,
            max_block_bytes: 64 * 1024,
            max_blocks: 4096,
            max_files: 16384,
            max_store_bytes: 256 * 1024 * 1024,
        }
    }
}
impl TransferLimits {
    /// Rejects zero and policies outside this beta's hard limits.
    pub fn validate(&self) -> TransferResult<()> {
        if self.max_object_bytes == 0
            || self.max_object_bytes > 256 * 1024 * 1024
            || self.max_block_bytes == 0
            || self.max_block_bytes > 1024 * 1024
            || self.max_blocks == 0
            || self.max_blocks > 4096
            || self.max_files < 4
            || self.max_files > 65536
            || self.max_store_bytes == 0
        {
            return Err(E::InvalidContract);
        }
        Ok(())
    }
}
/// Complete object plus exact ordered block hashes. A digest is not authenticity.
/// Deployment must authenticate this descriptor before admitting a transfer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "DescriptorWire")]
pub struct ObjectDescriptor {
    format_version: u16,
    object_hash: String,
    total_bytes: u64,
    block_bytes: u32,
    block_hashes: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DescriptorWire {
    format_version: u16,
    object_hash: String,
    total_bytes: u64,
    block_bytes: u32,
    #[serde(deserialize_with = "deserialize_hashes")]
    block_hashes: Vec<String>,
}
impl TryFrom<DescriptorWire> for ObjectDescriptor {
    type Error = E;
    fn try_from(wire: DescriptorWire) -> TransferResult<Self> {
        let value = Self {
            format_version: wire.format_version,
            object_hash: wire.object_hash,
            total_bytes: wire.total_bytes,
            block_bytes: wire.block_bytes,
            block_hashes: wire.block_hashes,
        };
        value.validate(TransferLimits {
            max_object_bytes: 256 * 1024 * 1024,
            max_block_bytes: 1024 * 1024,
            ..TransferLimits::default()
        })?;
        Ok(value)
    }
}
impl ObjectDescriptor {
    /// Constructs a version-one descriptor with strict lowercase SHA-256 identities.
    pub fn new(
        object_hash: String,
        total_bytes: u64,
        block_bytes: u32,
        block_hashes: Vec<String>,
        limits: TransferLimits,
    ) -> TransferResult<Self> {
        let value = Self {
            format_version: 1,
            object_hash,
            total_bytes,
            block_bytes,
            block_hashes,
        };
        value.validate(limits)?;
        Ok(value)
    }
    /// Reads an exact bounded object incrementally; no complete-object allocation.
    /// Callers own source I/O deadlines and authenticity of the resulting descriptor.
    pub fn from_reader(
        mut reader: impl Read,
        total_bytes: u64,
        block_bytes: u32,
        limits: TransferLimits,
    ) -> TransferResult<Self> {
        limits.validate()?;
        if total_bytes == 0
            || total_bytes > limits.max_object_bytes
            || block_bytes == 0
            || block_bytes > limits.max_block_bytes
        {
            return Err(E::Limit);
        }
        let blocks = ((total_bytes - 1) / u64::from(block_bytes) + 1) as usize;
        if blocks > limits.max_blocks {
            return Err(E::Limit);
        }
        let mut full = Sha256::new();
        let mut hashes = Vec::with_capacity(blocks);
        let mut remaining = total_bytes;
        let mut buffer = vec![0; block_bytes as usize];
        while remaining > 0 {
            let length = remaining.min(u64::from(block_bytes)) as usize;
            reader
                .read_exact(&mut buffer[..length])
                .map_err(|_| E::Source)?;
            full.update(&buffer[..length]);
            hashes.push(hash(&buffer[..length]));
            remaining -= length as u64;
        }
        if reader.read(&mut [0]).map_err(|_| E::Source)? != 0 {
            return Err(E::Integrity);
        }
        Self::new(
            hex(&full.finalize()),
            total_bytes,
            block_bytes,
            hashes,
            limits,
        )
    }
    /// Validates version, arithmetic and all resource/identity bounds.
    pub fn validate(&self, limits: TransferLimits) -> TransferResult<()> {
        limits.validate()?;
        if self.format_version != 1 {
            return Err(E::UnsupportedVersion);
        }
        if self.total_bytes == 0
            || self.total_bytes > limits.max_object_bytes
            || self.block_bytes == 0
            || self.block_bytes > limits.max_block_bytes
        {
            return Err(E::Limit);
        }
        let expected = ((self.total_bytes - 1) / u64::from(self.block_bytes) + 1) as usize;
        if expected > limits.max_blocks || self.block_hashes.len() != expected {
            return Err(E::Limit);
        }
        if !valid_hash(&self.object_hash) || self.block_hashes.iter().any(|h| !valid_hash(h)) {
            return Err(E::InvalidContract);
        }
        Ok(())
    }
    /// Strict bounded JSON; unknown fields/versions fail at this boundary.
    pub fn from_json(bytes: &[u8], limits: TransferLimits) -> TransferResult<Self> {
        if bytes.len() > 1024 * 1024 {
            return Err(E::Limit);
        }
        let wire: DescriptorWire = serde_json::from_slice(bytes).map_err(|_| E::InvalidContract)?;
        let value = Self::try_from(wire)?;
        value.validate(limits)?;
        Ok(value)
    }
    /// Deterministic descriptor serialization, bounded by the validated hash inventory.
    pub fn to_json(&self) -> TransferResult<Vec<u8>> {
        serde_json::to_vec(self).map_err(|_| E::InvalidContract)
    }
    /// Full object SHA-256 identity.
    pub fn object_hash(&self) -> &str {
        &self.object_hash
    }
    /// Exact complete object bytes.
    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }
    /// Maximum block length; the last block may be shorter.
    pub fn block_bytes(&self) -> u32 {
        self.block_bytes
    }
    /// Number of complete/incomplete blocks.
    pub fn block_count(&self) -> usize {
        self.block_hashes.len()
    }
    /// Exact length for a valid block index.
    pub fn block_len(&self, index: usize) -> TransferResult<usize> {
        if index >= self.block_count() {
            return Err(E::InvalidContract);
        }
        Ok(
            (self.total_bytes - index as u64 * u64::from(self.block_bytes))
                .min(u64::from(self.block_bytes)) as usize,
        )
    }
    /// Offset for a validated block index.
    pub fn block_offset(&self, index: usize) -> TransferResult<u64> {
        self.block_len(index)?;
        Ok(index as u64 * u64::from(self.block_bytes))
    }
    /// Verifies length and digest against the trusted inventory.
    pub fn verify_block(&self, index: usize, bytes: &[u8]) -> TransferResult<()> {
        if bytes.len() != self.block_len(index)? || hash(bytes) != self.block_hashes[index] {
            return Err(E::Integrity);
        }
        Ok(())
    }
}
pub(crate) fn hash(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}
pub(crate) fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn deserialize_hashes<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<String>, D::Error> {
    struct Hashes;
    impl<'de> serde::de::Visitor<'de> for Hashes {
        type Value = Vec<String>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("at most 4096 SHA-256 hashes")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            let mut hashes = Vec::new();
            while let Some(value) = seq.next_element::<String>()? {
                if hashes.len() >= 4096 || !valid_hash(&value) {
                    return Err(serde::de::Error::custom("invalid block hash inventory"));
                }
                hashes.push(value);
            }
            Ok(hashes)
        }
    }
    deserializer.deserialize_seq(Hashes)
}
