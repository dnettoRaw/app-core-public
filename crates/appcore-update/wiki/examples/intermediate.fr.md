# Activer, verifier le health et faire rollback

[English](intermediate.en.md) | [Português](intermediate.pt.md) |
[Exemple minimal](basic.fr.md) | [Guide](../guide.fr.md)

Commitez un artefact sain, activez son remplacement puis effectuez un rollback
si le health gate echoue.

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

Le flux de production doit verifier l'authenticite et executer un smoke test
borne avant activation, puis un vrai health gate avant commit.

Exemple supplémentaire de contrat borné ; exécutez dans un contexte retournant Result avec les dépendances explicitement choisies.

```rust
use appcore_update::{HostUpdateFlow, ActivationRequest, StagedArtifact, UpdateResult};
fn activate(flow: &HostUpdateFlow<'_>, request: &ActivationRequest, staged: &StagedArtifact) -> UpdateResult<()> {
    flow.prepare(request, staged, 100)?;
    let evidence = flow.activate(request, staged, 101)?;
    flow.confirm(&evidence, 102)
}
```

## Reprendre sans répéter l’installation

Fournir uniquement octets/signature installés et tentative/instance/snapshot exacts vérifiés indépendamment. Le host possède inspection bornée et lock/fence exclusif original. Conserver données de rollback et écritures bloquées dans l’incertitude. Après confirm, suivre le profil host pour finalisation distante/libération ; un container committed ne restaure pas d’anciens volumes seulement pour un report final perdu.

```rust
use appcore_update::{ActivationEvidence, HostUpdateFlow, UpdateResult};

// Hold the original installation lock/fence; ambiguous inspection stays blocked.
fn resume_observed(flow: &HostUpdateFlow<'_>, observed: &ActivationEvidence,
    observed_at_ms: u64, health_at_ms: u64) -> UpdateResult<()> {
    flow.reconcile_activated(observed, observed_at_ms)?;
    // Executes the adapter's real health gate before commit; never reinstalls.
    flow.confirm(observed, health_at_ms)
}
```
