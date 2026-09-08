# appcore-contracts

[English](README.en.md) | [Français](README.fr.md)

`appcore-contracts` define os documentos versionados trocados entre aplicações,
deployments e hosts AppCore em execução. Ele permite usar manifestos e policies
sem importar uma implementação do Runtime.

## O que ele contém

- `ApplicationManifestV1`: identidade portátil, módulos, capabilities e
  requisitos de Runtime da aplicação;
- `DeploymentManifestV1`: modo da instalação, providers, rede, TLS, volumes,
  ambiente, referências de segredo, Supervisor e watchdog;
- `RuntimeManifestV1`: identidade, modo operacional e saúde publicados por um
  host em execução;
- identificadores validados e policies de recursos, storage, liderança,
  agendamento, jobs, saúde e updates.

Os três manifestos têm donos diferentes. A aplicação publica o Application
Manifest, o operador fornece o Deployment Manifest e o host produz o Runtime
Manifest. Essa separação impede que configuração específica de uma máquina
entre no artefato portátil da aplicação.

## Quando usar

Use este crate para construir, analisar, validar ou inspecionar contratos V1 do
AppCore. Os construtores e `validate` rejeitam identificadores inválidos,
requisitos ausentes e combinações incoerentes de policies.

```rust
use appcore_contracts::{
    ApplicationId, ApplicationManifestV1, ContractResult,
    RuntimeRequirements, ServiceId,
};

fn manifesto() -> ContractResult<ApplicationManifestV1> {
    ApplicationManifestV1::new(
        ApplicationId::new("notes-app")?,
        "1.0.0",
        "Notes",
        "example-vendor",
        ServiceId::new("notes")?,
        RuntimeRequirements::new("1.0.0", "1")?,
    )
}
```

## Limites

Este crate de contratos não contém I/O, implementação de provider, listener,
ciclo de vida de processo ou schema de negócio. Os nomes serializados de V1
formam uma barreira de compatibilidade: adições compatíveis podem evoluir em
V1, mas entradas removidas ou incompatíveis não são adivinhadas nem convertidas
silenciosamente.

Consulte o [exemplo básico](wiki/examples/basic.pt.md), o
[exemplo intermediário](wiki/examples/intermediate.pt.md) e o
[guia do crate](wiki/guide.pt.md). Execute os testes focados com:

```bash
cargo test -p appcore-contracts
```

## Documentação estável

ID estável: **ACR-002**. Consulte o
[guia complementar de arquitetura e integração](https://wiki.appcore.dnettoraw.com/pt/crates/id/acr-002). Esse ID permanente
continua válido se a página da wiki mudar.
