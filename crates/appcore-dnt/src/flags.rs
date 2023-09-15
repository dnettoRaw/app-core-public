// =============================================================================
//        #######
//     ###       ###     F: flags.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/02 10:29:16 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 10:29:16 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Authenticated DNT flag partitioning and builders.

use crate::model_types::DntSealOptions;
use crate::{DntError, DntResult};

/// Internal DNT flag: stored encoded payload uses zlib-wrapped DEFLATE.
pub const DNT_FLAG_PAYLOAD_DEFLATE: u32 = 0x0000_0001;

/// Bits reserved for DNT envelope semantics.
pub const DNT_INTERNAL_FLAG_MASK: u32 = 0x0000_FFFF;
/// Bits available for caller/application semantics.
pub const DNT_USER_FLAG_MASK: u32 = 0xFFFF_0000;
/// First user flag bit in the raw DNT flag field.
pub const DNT_USER_FLAG_OFFSET: u8 = 16;
/// Number of user flag bits available to callers.
pub const DNT_USER_FLAG_COUNT: u8 = 16;

const KNOWN_INTERNAL_FLAGS: u32 = DNT_FLAG_PAYLOAD_DEFLATE;

/// Validated DNT flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DntFlags(u32);

impl DntFlags {
    /// Returns empty flags.
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Validates raw flag bits.
    pub fn from_bits(bits: u32) -> DntResult<Self> {
        validate_flags(bits)?;
        Ok(Self(bits))
    }

    /// Returns the raw header bits.
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Returns only DNT-owned internal bits.
    pub const fn internal_bits(self) -> u32 {
        self.0 & DNT_INTERNAL_FLAG_MASK
    }

    /// Returns only caller-owned user bits.
    pub const fn user_bits(self) -> u32 {
        self.0 & DNT_USER_FLAG_MASK
    }

    /// Enables compact DNT payload storage.
    pub const fn compact_payload(self) -> Self {
        Self(self.0 | DNT_FLAG_PAYLOAD_DEFLATE)
    }

    /// Adds one caller-owned user flag by relative index `0..16`.
    pub fn with_user_flag(self, index: u8) -> DntResult<Self> {
        Ok(Self(self.0 | dnt_user_flag(index)?))
    }
}

impl DntSealOptions {
    /// Enables compact payload storage for this write.
    ///
    /// The codec output is compressed with zlib-wrapped DEFLATE before AEAD
    /// encryption. The compression flag is part of the authenticated header.
    pub fn compact_payload(mut self) -> Self {
        self.flags |= DNT_FLAG_PAYLOAD_DEFLATE;
        self
    }

    /// Adds one caller-owned user flag by relative index `0..16`.
    pub fn with_user_flag(mut self, index: u8) -> DntResult<Self> {
        self.flags = DntFlags::from_bits(self.flags)?
            .with_user_flag(index)?
            .bits();
        Ok(self)
    }
}

/// Creates a caller-owned user flag by relative index `0..16`.
pub fn dnt_user_flag(index: u8) -> DntResult<u32> {
    if index >= DNT_USER_FLAG_COUNT {
        return Err(DntError::InvalidFlags);
    }
    1u32.checked_shl(u32::from(DNT_USER_FLAG_OFFSET + index))
        .ok_or(DntError::InvalidFlags)
}

/// Combines DNT-owned internal flags and caller-owned user flags.
pub fn dnt_compose_flags(internal_flags: u32, user_flags: u32) -> DntResult<u32> {
    if internal_flags & !KNOWN_INTERNAL_FLAGS != 0 || user_flags & !DNT_USER_FLAG_MASK != 0 {
        return Err(DntError::InvalidFlags);
    }
    Ok(internal_flags | user_flags)
}

pub(crate) fn validate_flags(flags: u32) -> DntResult<()> {
    let unknown_internal_flags = flags & DNT_INTERNAL_FLAG_MASK & !KNOWN_INTERNAL_FLAGS;
    if unknown_internal_flags != 0 {
        return Err(DntError::InvalidFlags);
    }
    Ok(())
}
