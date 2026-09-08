# appcore-provider

[English](README.en.md) | [Français](README.fr.md)

`appcore-provider` define como um Deployment Manifest seleciona infraestrutura
concreta do Runtime sem fazer crates de contrato dependerem de implementações.
Ele contém roles, planos de construção, factories, resolução de segredos,
coordenação genérica, leases compartilhados com fencing e contratos de jobs.

## Fluxo de composição

1. O deployment declara IDs e settings dos providers.
2. A raiz de composição registra implementações explícitas de `ProviderFactory`
   no `ProviderRegistry`.
3. `DeploymentProviderPlan` valida que cada `ProviderRole` obrigatório tenha
   exatamente um provider utilizável.
4. O factory recebe um `ProviderContext` limitado e owners dos segredos
   resolvidos, então devolve a implementação pedida.

Provider ausente ou inválido interrompe o bootstrap. O registry nunca escolhe
um substituto silenciosamente. `ResolvedSecret` zera os bytes que possui e não
deve ser copiado para diagnósticos.

```rust
use appcore_provider::{
    CoordinationStoreProvider, InMemoryCoordinationStore, ProviderResult,
};

fn verificar_coordenacao() -> ProviderResult<u64> {
    let store = InMemoryCoordinationStore::default();
    store.ensure_compatible()?;
    store.schema_version()
}
```

## Garantias de coordenação e lease

O store em memória serve para control planes embutidos ou de teste. O store em
arquivo usa schema V2, substituição atômica e limite de 4 KiB para metadata e
fontes de restore. Leitores verificam o comprimento antes de alocar, mantêm um
byte sentinela para detectar crescimento e rejeitam symlinks, arquivos não
regulares, UTF-8 inválido e registros corrompidos.

Leases em filesystem persistem um sidecar de maior epoch por recurso antes de
publicar o lease ativo. Liberar o lease nunca reinicia a sequência de fencing.
O token protege uma escrita somente se o writer verificar o epoch atual
imediatamente antes dela; filesystems sem lock, rename, sync de diretório ou
coerência de cache confiáveis não oferecem proteção forte contra split-brain.

Código específico de um provider pertence a um crate de integração. Consulte o
[exemplo básico](wiki/examples/basic.pt.md), o
[exemplo intermediário](wiki/examples/intermediate.pt.md) e o
[guia do crate](wiki/guide.pt.md).

```bash
cargo test -p appcore-provider
```

## Documentação estável

ID estável: **ACR-019**. Consulte o
[guia complementar de arquitetura e integração](https://wiki.appcore.dnettoraw.com/pt/crates/id/acr-019). Esse ID permanente
continua válido se a página da wiki mudar.
