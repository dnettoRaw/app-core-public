// =============================================================================
//        #######
//     ###       ###     F: bounded_wire.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/05 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/05 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Byte budgets at the bearer-token boundary, independent of HTTP ingress.

use std::io::{self, Write};

use super::{CommandTokenError, RuntimeTokenClaims};

/// Maximum decoded JSON claims size (64 KiB), including JSON escaping.
pub const MAX_RUNTIME_TOKEN_PAYLOAD_BYTES: usize = 64 * 1024;
/// Maximum provider signature size (256 KiB), including embedded payloads.
///
/// Providers may carry an authenticated payload rather than a fixed-size MAC.
pub const MAX_RUNTIME_TOKEN_SIGNATURE_BYTES: usize = 256 * 1024;
/// Maximum V1 envelope size: prefix, separator and two hex-encoded components.
pub const MAX_RUNTIME_TOKEN_BYTES: usize =
    4 + 2 * (MAX_RUNTIME_TOKEN_PAYLOAD_BYTES + MAX_RUNTIME_TOKEN_SIGNATURE_BYTES);

pub(super) fn validate_fields(fields: &[Option<&str>]) -> Result<(), CommandTokenError> {
    let mut remaining = MAX_RUNTIME_TOKEN_PAYLOAD_BYTES;
    for field in fields.iter().flatten() {
        remaining = remaining
            .checked_sub(field.len())
            .ok_or(CommandTokenError::InvalidFormat)?;
    }
    Ok(())
}

pub(super) fn validate_signature(signature: &[u8]) -> Result<(), CommandTokenError> {
    if signature.is_empty() || signature.len() > MAX_RUNTIME_TOKEN_SIGNATURE_BYTES {
        return Err(CommandTokenError::InvalidFormat);
    }
    Ok(())
}

pub(super) fn serialize(claims: &RuntimeTokenClaims) -> Result<Vec<u8>, serde_json::Error> {
    let mut writer = ClaimsWriter(Vec::new());
    serde_json::to_writer(&mut writer, claims)?;
    Ok(writer.0)
}

/// Rejects expansion before retaining it; JSON escaping must share the budget.
struct ClaimsWriter(Vec<u8>);

impl Write for ClaimsWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_RUNTIME_TOKEN_PAYLOAD_BYTES - self.0.len() {
            return Err(io::Error::other("Runtime token claims exceed byte limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
