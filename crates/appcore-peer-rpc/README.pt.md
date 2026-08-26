# appcore-peer-rpc

**Responsabilidade:** client peer autenticado, host HTTP, validação e replay
protection.

**Dependências internas:** core, distributed contracts, security e transport.

**API principal:** traits de token issuer/authenticator/dispatcher e
implementações HashToken/static; nonce stores memória/arquivo; config,
validator e hashes; retry/client config e transport trait; transportes pooled e
standard one-shot; HTTP state e host.

Use `PooledPeerRpcTransport` para reutilizar conexões limitadas por origem.
`StdPeerRpcTransport` preserva o transporte V1 one-shot.

O contrato opt-in `v2`, `PeerRpcChunkEncoder` e `PeerRpcChunkAssembler`
processam sources e sinks grandes usando um chunk limitado por vez. Os limites
default são 64 KiB decodificados por chunk, 96 KiB codificados, 64 MiB totais e
1.024 chunks. Sequência, tamanhos exatos, hash por chunk e total, deadline,
cancelamento e quota após descompressão falham fechados. Essas APIs de codec não
selecionam transporte V2 automaticamente; rotas V1 nunca inferem V2.

`PeerRpcStreamRegistry` adiciona quotas exatas de sessões e bytes decodificados,
spools exclusivos acessíveis somente pelo proprietário, pulls limitados para a
resposta do dispatcher e contadores de saturação/limpeza. Erro, cancelamento,
expiração e conclusão liberam o arquivo parcial e sua reserva.
Unix exige o proprietário efetivo com modos `0700`/`0600` no diretório/arquivo.
Windows rejeita reparse points e qualquer allow ACE fora do SID proprietário
do processo atual. Outras plataformas rejeitam a configuração do spool.

HTTP V2 é instalado somente por `PeerRpcHttpHost::with_v2_stream_registry`.
`query_stream_v2` e `command_stream_v2` vinculam cada body JSON exato a um novo
bearer token e processam request/response incrementalmente. O open reutiliza
validações de tenant, cluster, target, trace, deadline e nonce replay; commands
exigem idempotência. Frames não são repetidos após falha ambígua de transporte.
V1 continua sendo a superfície default e nunca faz upgrade automático.

[Evidência clean-source da certificação V2 de 64 MiB](wiki/benchmarks/peer-rpc-v2-2026-08-26.pt.md)

Use somente quando tenant, cluster, source, target, protocolo, expiry, nonce e
integridade podem ser provados. `AllowPeerAuthenticator` é somente teste.

O `Debug` dos DTOs peer request, response, outbound e HTTP mostra tamanhos e
omite bytes opacos, credenciais, valores de nonce/idempotencia e detalhes de
erro remoto.

**Maturidade:** V1 estável; transporte V2 pós-1.0 certificado em desenvolvimento,
ainda não publicado.
