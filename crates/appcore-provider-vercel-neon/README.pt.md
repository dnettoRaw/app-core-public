# appcore-provider-vercel-neon

Aquisição/renovação e liberação de lease usam uma tentativa HTTP, independentemente
da configuração de retries. V1 não fornece chave de deduplicação remota; a
resposta pode se perder após aplicar a mutação. Timeout ou falha HTTP transitória
não provam que o lease ficou inalterado. Reconcilie estado autoritativo e fencing
antes de escritas dependentes de liderança; não repita a operação cegamente.
Discovery, registro e heartbeat mantêm retries configurados; a semântica remota
ainda exige testes de conformidade do deployment.

A factory valida retries antes de resolver segredos: 1–16 tentativas, timeout
e backoffs entre 1 e 30.000 ms, com backoff inicial menor ou igual ao máximo.
A soma conservadora `timeout × tentativas + max_backoff × (tentativas − 1)`
não pode superar 120.000 ms. Configuração inválida é rejeitada, não ajustada
silenciosamente. Esse orçamento não é um deadline de relógio imposto.

[English](README.en.md) | [Français](README.fr.md)

`appcore-provider-vercel-neon` é o adapter oficial isolado que permite a um
deployment AppCore usar uma API de control plane hospedada na Vercel e apoiada
por um serviço Neon operado externamente. Os nodes do Runtime chamam HTTPS;
eles nunca conectam diretamente ao Neon.

## O que o crate fornece

- `VercelNeonControlPlaneFactory`, registrado como
  `VERCEL_NEON_PROVIDER_ID`;
- `SharedControlPlaneProvider`, o tipo entregue à raiz de composição;
- `AUTH_TOKEN_SECRET`, o slot exato de segredo esperado pelo factory;
- validação do endpoint, settings e bearer token resolvido antes de liberar o
  client de control plane.

O deployment seleciona o provider explicitamente e fornece um endpoint HTTPS
com uma referência de segredo:

```rust
use appcore_contracts::{ProviderConfig, ProviderId, SecretRef};
use appcore_provider_vercel_neon::{
    AUTH_TOKEN_SECRET, VERCEL_NEON_PROVIDER_ID,
};

let config = ProviderConfig::new(ProviderId::new(VERCEL_NEON_PROVIDER_ID)?)
    .with_endpoint("https://control.example.com")?
    .with_secret_ref(
        AUTH_TOKEN_SECRET,
        SecretRef::new("env:APPCORE_CONTROL_TOKEN")?,
    )?
    .with_setting("timeout_ms", "5000")?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Fronteira de confiança

O Deployment Manifest do Runtime contém somente o endpoint da Vercel e uma
referência de token. Strings de conexão do Neon, credenciais de banco,
migrações de schema, backup e retenção ficam no serviço operado separadamente.
Segredo ausente, endpoint sem HTTPS, setting inválido, falha de autenticação ou
serviço remoto indisponível falham explicitamente; este adapter não escolhe
outro provider como fallback.

Consulte o [exemplo básico](wiki/examples/basic.pt.md), o
[exemplo intermediário](wiki/examples/intermediate.pt.md) e o
[guia do crate](wiki/guide.pt.md). Execute:

```bash
cargo test -p appcore-provider-vercel-neon
```

## Documentação estável

ID estável: **ACR-020**. Consulte o
[guia complementar de arquitetura e integração](https://wiki.appcore.dnettoraw.com/pt/crates/id/acr-020). Esse ID permanente
continua válido se a página da wiki mudar.
