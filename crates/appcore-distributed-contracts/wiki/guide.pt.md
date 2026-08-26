# appcore-distributed-contracts

[Exemplo minimo](examples/basic.pt.md) |
[Exemplo intermediario](examples/intermediate.pt.md)

**Responsabilidade:** contratos wire/provider versionados de control plane e
peer RPC.

**Dependências internas:** `appcore-contracts`, `appcore-types`.

**API principal:** constantes e paths do protocolo, registration, presence,
heartbeat, peer directory, leases de compatibilidade, leases por serviço,
leadership decisions e traits; paths peer, envelopes, responses, errors, call
kinds, advertisement DTOs, client executor e metadados de transporte para
content-envelope opaco.

Implementações pertencem aos crates de control plane ou peer. Não adicione
cliente HTTP, filesystem, tokens ou regras de capability de produto.

A serializacao wire de opaque-content e Peer RPC nao muda. O `Debug` mostra
tamanhos e metadata de roteamento, sem bytes de payload opaco, valores de
nonce/idempotencia ou detalhes de erro remoto.

`peer_rpc::v2` é uma família independente e opt-in de frames. O frame open fixa
quota agregada, tamanho/quantidade de chunks e deadline; chunks carregam
sequência, encoding, tamanho decodificado e digest exatos; commit vincula
tamanho e digest totais; cancel usa motivo controlado. Bytes codificados usam
string JSON base64 canônica, não array de inteiros. V1 e V2 possuem módulos
e rotas separados. Nenhum parser detecta, atualiza ou faz fallback entre eles.

**Maturidade:** V1 estável; contrato de chunks V2 pós-1.0 em desenvolvimento.
