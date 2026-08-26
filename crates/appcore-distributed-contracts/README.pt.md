# appcore-distributed-contracts

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

Peer RPC V2 é uma família separada e opt-in de frames em `peer_rpc::v2`.
Frames open, chunk, commit e cancel declaram protocolo, identidade, sequência,
tamanhos decodificados, deadline e integridade exatos. Bytes codificados usam
uma string JSON base64 canônica, nunca array de inteiros. V1 permanece somente em
`peer_rpc::v1`; implementações nunca podem inferir ou converter entre versões.

**Maturidade:** V1 estável; contrato de chunks V2 em desenvolvimento pós-1.0.
