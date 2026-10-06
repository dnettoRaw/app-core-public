# appcore-transfer — intermediate

Beta `0.1.0-beta.1` transfers immutable opaque objects from explicitly approved sources. Descriptors must come from a trusted application or deployment. SHA-256 integrity does not authenticate a release.

```rust
use appcore_transfer::{ObjectDescriptor, TransferLimits, TransferStore};
use std::{io::Cursor, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Deployment creates and protects this root before invocation.
    let root = PathBuf::from(std::env::args_os().nth(1).ok_or("private absolute root required")?);
    let bytes = b"bounded immutable object";
    let limits = TransferLimits::default();
    let descriptor = ObjectDescriptor::from_reader(Cursor::new(bytes), bytes.len() as u64, 8, limits)?;
    let mut store = TransferStore::open(root, limits)?;
    let mut session = store.begin(descriptor)?;
    for index in session.missing() { session.accept(index, &bytes[index * 8..(index * 8 + 8).min(bytes.len())])?; }
    let object = session.finish()?;
    assert_eq!(object.descriptor().total_bytes(), bytes.len() as u64);
    Ok(())
}
```

A private existing root is required: Unix owner-only 0700, or a Windows owner-only ACL. The store holds a cross-process exclusive lock. Verified blocks survive restart; corruption and unknown versions fail closed. Recognized orphan staging files are removed under the lock. TTL cleanup removes only incomplete objects. Complete objects remain until deployment-owned retention.