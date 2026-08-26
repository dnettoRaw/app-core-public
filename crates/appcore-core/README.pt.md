# appcore-core

**Responsabilidade:** lifecycle, registro, dispatch, state, audit e idempotência
genéricos dentro do processo.

**Dependências internas:** `appcore-contracts`, `appcore-types`.

**API principal:** `RuntimeBuilder`, `RuntimeController`, `RuntimeInstance`,
`RuntimeLifecycle`, registries e buses de command/event, envelopes,
`CommandHandler`, `CommandResult`, `RuntimeContext`, audit log/journal,
idempotência em memória/arquivo, state e decision engines, clock, redaction e
`AppPlugin` de compatibilidade.

Clones de `RuntimeController` compartilham lifecycle, idempotência e comandos
em execução. O command bus imutável possui handlers por `Arc`. Handlers
independentes podem executar em paralelo, enquanto uma mesma chave idempotente
admite no máximo uma execução. O shutdown fecha a admissão atomicamente e
permite drenagem limitada dos comandos já admitidos.

Aplicações novas usam re-exports de `appcore_bin::application`; não montam o
core manualmente. Mantenha I/O adapters e comportamento de domínio fora.

**Maturidade:** superfície low-level RC estável; builder/plugin são de
compatibilidade e manifest-first é o caminho preferido.
