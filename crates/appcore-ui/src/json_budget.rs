// =============================================================================
//        #######
//     ###       ###     F: json_budget.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Measures serialized JSON before retention without materializing an extra body.
use crate::{UiError, UiResult};
use serde::Serialize;
use std::io::{self, Write};
pub(crate) fn json_bytes(value: &impl Serialize, limit: usize) -> UiResult<usize> {
    let mut counter = Counter {
        remaining: limit,
        exceeded: false,
    };
    let encoded = serde_json::to_writer(&mut counter, value);
    if counter.exceeded {
        return Err(UiError::PayloadTooLarge);
    }
    encoded.map_err(|_| UiError::InvalidFrame)?;
    Ok(limit - counter.remaining)
}
struct Counter {
    remaining: usize,
    exceeded: bool,
}
impl Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.remaining {
            self.exceeded = true;
            return Err(io::Error::other("UI JSON budget exceeded"));
        }
        self.remaining -= bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
