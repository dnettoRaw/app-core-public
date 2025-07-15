# appcore-peer-rpc

[Exemplo minimo](examples/basic.pt.md) |
[Exemplo intermediario](examples/intermediate.pt.md)

**Responsabilidade:** client peer autenticado, host HTTP, validação e replay
protection.

**Dependências internas:** core, distributed contracts, security e transport.

**API principal:** traits de token issuer/authenticator/dispatcher e
implementações HashToken/static; nonce stores memória/arquivo; config,
validator e hashes; retry/client config e transport trait; transporte standard;
HTTP state e host.

Use somente quando tenant, cluster, source, target, protocolo, expiry, nonce e
integridade podem ser provados. `AllowPeerAuthenticator` é somente teste.

O `Debug` dos DTOs peer request, response, outbound e HTTP mostra tamanhos e
omite bytes opacos, credenciais, valores de nonce/idempotencia e detalhes de
erro remoto.

**Maturidade:** superfície peer V1 RC estável.
