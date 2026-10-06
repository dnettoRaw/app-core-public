# appcore-transfer — intermediate

La beta `0.1.0-beta.1` transfère des objets opaques immuables depuis des sources explicitement autorisées. Les descripteurs proviennent d’une application ou d’un déploiement de confiance. L’intégrité SHA-256 n’authentifie pas une release.

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

Une racine privée existante est obligatoire : Unix 0700 du propriétaire, ou ACL Windows réservée au propriétaire. Le stockage conserve un verrou exclusif interprocessus. Les blocs vérifiés survivent au redémarrage ; corruption et versions inconnues échouent explicitement. Les fichiers temporaires orphelins reconnus sont supprimés sous verrou. Le TTL supprime uniquement les objets incomplets. La rétention des objets complets appartient au déploiement.