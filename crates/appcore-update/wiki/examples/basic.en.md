# Minimal artifact compatibility check

[Português](basic.pt.md) | [Français](basic.fr.md) |
[Intermediate example](intermediate.en.md) | [Guide](../guide.en.md)

Describe an immutable artifact and reject it before download when Runtime or
protocol compatibility does not match.

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

The checksum and size describe the bytes to be fetched. Authenticity still
requires an accepted signing key and signature policy.

Additional bounded contract example; execute in a Result-returning context with explicitly selected dependencies.

```rust
use appcore_update::{HostUpdateFlow, ActivationRequest, StagedArtifact, UpdateResult};
fn activate(flow: &HostUpdateFlow<'_>, request: &ActivationRequest, staged: &StagedArtifact) -> UpdateResult<()> {
    flow.prepare(request, staged, 100)?;
    let evidence = flow.activate(request, staged, 101)?;
    flow.confirm(&evidence, 102)
}
```
