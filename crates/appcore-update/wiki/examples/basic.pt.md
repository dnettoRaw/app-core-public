# Verificacao minima de compatibilidade de artefato

[English](basic.en.md) | [Français](basic.fr.md) |
[Exemplo intermediario](intermediate.pt.md) | [Guia](../guide.pt.md)

Descreva um artefato imutavel e rejeite-o antes do download quando Runtime ou
protocolo nao forem compativeis.

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

Checksum e tamanho descrevem os bytes a buscar. Autenticidade ainda exige uma
chave de assinatura aceita e politica de assinatura.

Exemplo adicional de contrato limitado; execute num contexto que retorna Result com dependências explicitamente selecionadas.

```rust
use appcore_update::{HostUpdateFlow, ActivationRequest, StagedArtifact, UpdateResult};
fn activate(flow: &HostUpdateFlow<'_>, request: &ActivationRequest, staged: &StagedArtifact) -> UpdateResult<()> {
    flow.prepare(request, staged, 100)?;
    let evidence = flow.activate(request, staged, 101)?;
    flow.confirm(&evidence, 102)
}
```
