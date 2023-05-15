# Minimal application contract

[Português](basic.pt.md) | [Français](basic.fr.md) |
[Intermediate example](intermediate.en.md) | [Guide](../guide.en.md)

Construct the smallest valid V1 Application Manifest directly from typed
identifiers and Runtime requirements.

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

The constructors reject malformed identifiers and incomplete versioned
contracts. Business schemas do not belong in this manifest.
