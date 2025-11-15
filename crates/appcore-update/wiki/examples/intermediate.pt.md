# Ativar, verificar health e fazer rollback

[English](intermediate.en.md) | [Français](intermediate.fr.md) |
[Exemplo minimo](basic.pt.md) | [Guia](../guide.pt.md)

Confirme um artefato saudavel, ative seu substituto e faca rollback quando o
health gate falhar.

```rust
use appcore_contracts::{ApplicationId, BuildId};
use appcore_update::{
    ArtifactDescriptor, ArtifactStore, FileArtifactStore,
};
use std::error::Error;

const BYTES: &[u8] = b"abc";
const SHA256: &str =
    "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

fn descriptor(version: &str, build: &str) -> Result<ArtifactDescriptor, Box<dyn Error>> {
    Ok(ArtifactDescriptor::new(
        ApplicationId::new("notes-app")?,
        version,
        BuildId::new(build)?,
        "stable",
        ">=1.0.0, <2.0.0",
        "1",
        format!("memory:{build}"),
        SHA256,
        BYTES.len() as u64,
    )?)
}

fn main() -> Result<(), Box<dyn Error>> {
    let root = std::env::temp_dir().join(format!(
        "appcore-update-example-{}",
        std::process::id()
    ));
    let store = FileArtifactStore::new(&root);

    let stable = descriptor("1.3.0", "notes-1-3-0")?;
    stable.ensure_compatible("1.0.0", "1")?;
    let stable_receipt = store.activate(store.stage(&stable, BYTES)?)?;
    store.commit(&stable_receipt)?;

    let candidate = descriptor("1.4.0", "notes-1-4-0")?;
    candidate.ensure_compatible("1.0.0", "1")?;
    let candidate_receipt = store.activate(store.stage(&candidate, BYTES)?)?;
    let health_gate_passed = false;
    if health_gate_passed {
        store.commit(&candidate_receipt)?;
    } else {
        store.rollback(&candidate_receipt)?;
    }

    let active = store.current()?.ok_or("active artifact missing")?;
    println!("active build={}", active.build_id().as_str());
    std::fs::remove_dir_all(root)?;
    Ok(())
}
```

O fluxo de producao deve verificar autenticidade e executar smoke test limitado
antes da ativacao, usando depois um health gate real antes do commit.
