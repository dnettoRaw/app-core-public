# Contrat applicatif minimal

[English](basic.en.md) | [Português](basic.pt.md) |
[Exemple intermediaire](intermediate.fr.md) | [Guide](../guide.fr.md)

Construisez le plus petit Application Manifest V1 valide avec des identifiants
types et les exigences du Runtime.

```rust
use appcore_contracts::{
    ApplicationId, ApplicationManifestV1, ContractResult, RuntimeRequirements,
    ServiceId,
};

fn main() -> ContractResult<()> {
    let manifest = ApplicationManifestV1::new(
        ApplicationId::new("notes-app")?,
        "1.0.0",
        "Notes",
        "example-vendor",
        ServiceId::new("notes")?,
        RuntimeRequirements::new("1.0.0", "1")?,
    )?;

    manifest.validate()?;
    println!("{} {}", manifest.application_id(), manifest.application_version());
    Ok(())
}
```

Les constructeurs refusent les identifiants mal formes et les contrats
versionnes incomplets. Les schemas metier n'appartiennent pas a ce manifest.
