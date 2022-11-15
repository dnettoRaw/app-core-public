# appcore-types

[Exemplo minimo](examples/basic.pt.md) |
[Exemplo intermediario](examples/intermediate.pt.md)

**Responsabilidade:** identificadores validados, identity e trace compartilhados
pelos contratos.

**Dependências internas:** `appcore-contracts`.

**API principal:** IDs de application, node, tenant, cluster, Core, instance,
command, event, query, state e capability; `RuntimeIdentity`, `CoreIdentity`,
policies/status de versão, `TraceContext`, `RuntimeError`,
`RuntimeResult`.

Use esses tipos em vez de strings não validadas nas fronteiras. Não coloque
estado de implementação, I/O ou comportamento de provider aqui.

**Maturidade:** superfície fundamental RC estável.
