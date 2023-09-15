// =============================================================================
//        #######
//     ###       ###     F: ids.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/02 00:04:12 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 00:04:12 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! DNT identifier types.

use crate::{DntError, DntResult};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Logical payload content type stored in the authenticated header.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContentType(String);

/// Payload codec identifier stored in the authenticated header.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CodecId(String);

/// Rotation-aware key identifier stored in the authenticated header.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct KeyId(String);

macro_rules! impl_dnt_id {
    ($ty:ident, $field:literal, $max:expr, $allow_slash:expr) => {
        impl $ty {
            /// Creates and validates an identifier.
            pub fn new(value: impl Into<String>) -> DntResult<Self> {
                let value = value.into();
                validate_id($field, &value, $max, $allow_slash)?;
                Ok(Self(value))
            }

            /// Returns the identifier as a string slice.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Debug for $ty {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter
                    .debug_tuple(stringify!($ty))
                    .field(&self.0)
                    .finish()
            }
        }
    };
}

impl_dnt_id!(ContentType, "content_type", 128, true);
impl_dnt_id!(CodecId, "codec_id", 64, false);
impl_dnt_id!(KeyId, "key_id", 128, false);

fn validate_id(_field: &'static str, value: &str, max: usize, allow_slash: bool) -> DntResult<()> {
    if value.is_empty() || value.len() > max {
        return Err(DntError::InvalidFormat);
    }
    let valid = value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric()
            || matches!(byte, b'.' | b'-' | b'_')
            || (allow_slash && byte == b'/')
    });
    if valid && !value.starts_with('/') && !value.ends_with('/') {
        return Ok(());
    }
    Err(DntError::InvalidFormat)
}
