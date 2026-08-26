# appcore-control-plane

**Responsabilidade:** implementações genéricas de presença, heartbeat, discovery
e leases.

**Dependências internas:** contracts, core, distributed contracts e transport.

**API principal:** clients in-memory, file e offline; configuração HTTP, retry
policy e transport trait; transports one-shot standard, pooled e bearer;
coordinator e heartbeat policy; guards de liderança global/serviço; validação
de endpoint seguro.

`PooledHttpTransport` é o perfil HTTP reutilizável sem autenticação e
`BearerHttpTransport` também reutiliza seu cliente limitado.
`StdHttpTransport` preserva o perfil V1 one-shot.

Use para coordenação distribuída sem payload de negócio. Perfil file exige
locks/storage certificados. Perfil remoto exige TLS e autenticação do
deployment.

O perfil file limita estado e backup a 16 MiB e rejeita estado malformado ou
futuro. A aritmética de expiração e epoch é verificada; o esgotamento do epoch
falha fechado em vez de reutilizar um fencing token.

**Maturidade:** contratos e referências RC estáveis; operação do serviço
externo pertence ao deployment.
