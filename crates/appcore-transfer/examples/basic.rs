// =============================================================================
//        #######
//     ###       ###     F: basic.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/06 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/06 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use appcore_transfer::{ObjectDescriptor, TransferLimits};
use std::io::Cursor;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = b"opaque bytes";
    let limits = TransferLimits::default();
    let descriptor =
        ObjectDescriptor::from_reader(Cursor::new(bytes), bytes.len() as u64, 8, limits)?;
    let recovered = ObjectDescriptor::from_json(&descriptor.to_json()?, limits)?;
    assert_eq!(descriptor, recovered);
    Ok(())
}
