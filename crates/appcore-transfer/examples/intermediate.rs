// =============================================================================
//        #######
//     ###       ###     F: intermediate.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/06 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/06 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use appcore_transfer::{ObjectDescriptor, TransferLimits, TransferStore};
use std::{io::Cursor, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Deployment creates and protects this root before invocation.
    let root = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("private absolute root required")?,
    );
    let bytes = b"bounded immutable object";
    let limits = TransferLimits::default();
    let descriptor =
        ObjectDescriptor::from_reader(Cursor::new(bytes), bytes.len() as u64, 8, limits)?;
    let mut store = TransferStore::open(root, limits)?;
    let mut session = store.begin(descriptor)?;
    for index in session.missing() {
        session.accept(index, &bytes[index * 8..(index * 8 + 8).min(bytes.len())])?;
    }
    let object = session.finish()?;
    assert_eq!(object.descriptor().total_bytes(), bytes.len() as u64);
    Ok(())
}
