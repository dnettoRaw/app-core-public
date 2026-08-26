# appcore-control-plane

[Exemplo minimo](examples/basic.pt.md) |
[Exemplo intermediario](examples/intermediate.pt.md)

**Responsabilidade:** implementações genéricas de presença, heartbeat, discovery
e leases.

**Dependências internas:** contracts, core, distributed contracts e transport.

**API principal:** clients in-memory, file e offline; configuração HTTP, retry
policy e transport trait; transports one-shot standard, pooled e bearer;
coordinator e heartbeat policy; guards de liderança global/serviço; validação
de endpoint seguro.

Use `PooledHttpTransport` para chamadas reutilizáveis sem autenticação.
`BearerHttpTransport` também possui um cliente reutilizável e limitado.
Mantenha `StdHttpTransport` somente onde o comportamento V1 one-shot com
`Connection: close` for necessário.

Use para coordenação distribuída sem payload de negócio. Perfil file exige
locks/storage certificados. Perfil remoto exige TLS e autenticação do
deployment.

O perfil file limita estado e backup a 16 MiB e rejeita estado malformado ou
futuro. A aritmética de expiração e epoch é verificada; o esgotamento do epoch
falha fechado em vez de reutilizar um fencing token.

**Maturidade:** contratos e referências RC estáveis; operação do serviço
externo pertence ao deployment.
