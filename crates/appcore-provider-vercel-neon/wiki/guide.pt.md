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

[Exemplo minimo](examples/basic.pt.md) |
[Exemplo intermediario](examples/intermediate.pt.md)

**Responsabilidade:** factory oficial isolada do adapter Vercel API com
coordenação Neon operada externamente.

**Dependências internas:** contracts, control plane e provider.

**API principal:** `VERCEL_NEON_PROVIDER_ID`, `AUTH_TOKEN_SECRET`, tipo shared
do client e `VercelNeonControlPlaneFactory`.

Nodes recebem somente endpoint Vercel e referência do auth token. Credenciais,
schema, backup e retention Neon ficam no serviço externo.

**Maturidade:** adapter RC suportado; certificação inclui o backend separado.
