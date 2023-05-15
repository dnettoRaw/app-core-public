# Contrato minimo de aplicacao

[English](basic.en.md) | [Français](basic.fr.md) |
[Exemplo intermediario](intermediate.pt.md) | [Guia](../guide.pt.md)

Construa o menor Application Manifest V1 valido diretamente com identificadores
tipados e requisitos do Runtime.

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

Os construtores rejeitam identificadores malformados e contratos versionados
incompletos. Schemas de negocio nao pertencem a este manifest.
