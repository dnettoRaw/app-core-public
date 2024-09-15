// =============================================================================
//        #######
//     ###       ###     F: codec.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/02 13:08:16 by dnettoRaw
//    ##   ## ##   ##    U: 2026/06/04 11:57:41 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Internal hexadecimal codec shared by file and HTTP sync formats.

use crate::sync::error::{SyncError, SyncResult};

pub(crate) fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(hex_nibble(byte >> 4));
        out.push(hex_nibble(byte & 0x0f));
    }
    out
}

fn hex_nibble(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'a' + (value - 10)) as char,
        _ => '0',
    }
}

pub(crate) fn hex_to_bytes(value: &str) -> SyncResult<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return Err(SyncError::InvalidEventHex);
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for chunk in value.as_bytes().chunks(2) {
        let high = from_hex_char(chunk[0])?;
        let low = from_hex_char(chunk[1])?;
        bytes.push((high << 4) | low);
    }
    Ok(bytes)
}

fn from_hex_char(value: u8) -> SyncResult<u8> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(SyncError::InvalidEventHex),
    }
}
