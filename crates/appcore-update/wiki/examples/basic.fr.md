# Verification minimale de compatibilite d'un artefact

[English](basic.en.md) | [Português](basic.pt.md) |
[Exemple intermediaire](intermediate.fr.md) | [Guide](../guide.fr.md)

Decrivez un artefact immuable et refusez-le avant le download si le Runtime ou
le protocole n'est pas compatible.

```rust
use appcore_contracts::{ApplicationId, BuildId};
use appcore_update::ArtifactDescriptor;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let artifact = ArtifactDescriptor::new(
        ApplicationId::new("notes-app")?,
        "1.4.0",
        BuildId::new("notes-1-4-0-linux-x86-64")?,
        "stable",
        ">=1.0.0, <2.0.0",
        "1",
        "https://updates.example.com/notes/1.4.0/app",
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        3,
    )?;

    artifact.ensure_compatible("1.0.0", "1")?;
    println!("compatible build={}", artifact.build_id().as_str());
    Ok(())
}
```

Le checksum et la taille decrivent les octets a recuperer. L'authenticite exige
encore une cle de signature acceptee et une policy de signature.
