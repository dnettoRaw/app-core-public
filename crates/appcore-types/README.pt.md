# appcore-types

[English](README.en.md) | [Français](README.fr.md)

`appcore-types` fornece o vocabulário validado compartilhado pelos contratos e
componentes do Runtime AppCore. Ele substitui strings sem validação nas
fronteiras de processo, storage e protocolo por tipos pequenos que aplicam as
mesmas regras em todo lugar.

## Contratos principais

- identificadores de aplicação, tenant, cluster, node, Core, instância,
  capability, command, query, event, state e grupo de sync;
- `RuntimeIdentity` e `CoreIdentity`, incluindo policy e status de
  compatibilidade;
- `CapabilityDescriptor`, manifestos de Core distribuído e endpoints de peers;
- `TraceContext` para correlação de trace, span, parent, command, origem e
  tenant;
- `RuntimeError` e `RuntimeResult` para falhas fundamentais controladas.

Crie os identificadores na primeira fronteira não confiável e passe o valor
tipado dali em diante. Um trace filho preserva trace, tenant, origem e command,
mas recebe um novo span e o Core atual.

```rust
use appcore_types::{CoreId, RuntimeResult, TenantId, TraceContext};

fn trace_filho() -> RuntimeResult<TraceContext> {
    let api = CoreId::new("core-api")?;
    let raiz = TraceContext::new(
        "trace-42",
        "span-api",
        api.clone(),
        api,
        TenantId::new("tenant-a")?,
    )?;

    raiz.child_span("span-worker", CoreId::new("core-worker")?)
}
```

## Limites e falhas

Este crate não contém I/O, estado mutável do Runtime, comportamento de provider
ou modelo de negócio. Comprimento, caracteres, nomes reservados e identidades
incoerentes são rejeitados na construção, antes de entrarem em outro
subsistema. Versões de protocolo e contrato são valores explícitos; elas não
são inferidas de um peer de rede.

Consulte o [exemplo básico](wiki/examples/basic.pt.md), o
[exemplo intermediário](wiki/examples/intermediate.pt.md) e o
[guia do crate](wiki/guide.pt.md). Execute:

```bash
cargo test -p appcore-types
```

## Documentação estável

ID estável: **ACR-003**. Consulte o
[guia complementar de arquitetura e integração](https://wiki.appcore.dnettoraw.com/pt/crates/id/acr-003). Esse ID permanente
continua válido se a página da wiki mudar.
