# appcore-peer-rpc

A entrada de corpos V1, JSON V2 e binário V2 compartilha 16 slots por host,
inclusive entre routers obtidos separadamente desse host. Saturação retorna
HTTP 503 antes da coleta; a recepção expira após 10 segundos (408). V1 mantém
o teto HTTP de 2 MiB; V2 mantém o teto de frame do registry. Health e manifest
não passam pela admissão de corpos. Essas falhas são de transporte antes do
decode, não respostas V2 assinadas. Corpos brutos admitidos ficam limitados a
16 vezes o maior teto habilitado, não ao RSS total. Decode, descompressão,
dispatch e respostas possuem limites separados.

[Exemplo minimo](examples/basic.pt.md) |
[Exemplo intermediario](examples/intermediate.pt.md)

**Responsabilidade:** client peer autenticado, host HTTP, validação e replay
protection.

**Dependências internas:** core, distributed contracts, security e transport.

**API principal:** traits de token issuer/authenticator/dispatcher e
implementações HashToken/static; nonce stores memória/arquivo; config,
validator e hashes; retry/client config e transport trait; transportes pooled e
standard one-shot; HTTP state e host.

Use `PooledPeerRpcTransport` para reutilizar conexões limitadas por origem.
`StdPeerRpcTransport` preserva o comportamento V1 one-shot com
`Connection: close`.
Ambos consomem a alocação owned do body do DTO HTTP: um corpo V1 sem compressão
ou frame V2 exato mantém a mesma alocação `Vec<u8>` através de `HttpRequest`,
sem reter um body clonado ao lado dela.

O client V1 move um payload outbound owned para um envelope e mantém esse owner
durante os retries limitados. Cada retry ainda gera campos temporais e nonce
novos, assina o envelope renovado e codifica outro body HTTP. Um workload com
payload raw de 4 MiB em cinco processos release no Apple M1 manteve p50 em
+0,08%, reduziu RSS pico em 7,21%, delta RSS da workload em 9,02% e delta RSS
retido em 7,99%.

Na entrada, `decode_peer_rpc_envelope_json` aplica o teto de bytes codificados
antes do parse e empresta diretamente o body HTTP sem compressão. A composition
root do Runtime também move o payload V1 decodificado para `CommandEnvelope`.
Com payload raw de 4 MiB no Apple M1, cinco processos release reduziram o p50 de
53,06 para 52,35 ms, RSS pico de 35,80 para 23,81 MiB e o delta RSS da workload
em 39,52%.

Dispatch V1 e V2 compartilha 16 permits blocking no processo e um pool Tokio
com o mesmo teto, stacks de 1 MiB e remoção de ociosas em cinco segundos. Gate
cheio rejeita antes da fila; V2 usa `CapacityExceeded`.

Use somente quando tenant, cluster, source, target, protocolo, expiry, nonce e
integridade podem ser provados. `AllowPeerAuthenticator` é somente teste.

O `Debug` dos DTOs peer request, response, outbound e HTTP mostra tamanhos e
omite bytes opacos, credenciais, valores de nonce/idempotencia e detalhes de
erro remoto.

Use `FilePeerNonceStore` somente em seu diretório privado do owner. Ele aceita
no máximo 65.536 entradas vivas, limita cada nonce a 128 bytes e o arquivo V1 a
16 MiB. O load decodifica direto do reader limitado; cada request aceito
reescreve o mapa ordenado por buffer fixo de 64 KiB e troca atômica, sem um
`Vec` JSON codificado. O startup rejeita campos desconhecidos, chaves inválidas,
corrupção completa e arquivos oversized. O benchmark do crate valida o máximo
de entradas com fases RSS idle/workload/retained.

`BoundedReplayStore` aplica a mesma validação de nonce à proteção de replay em
memória do processo. Quantidade e bytes retidos estimados possuem limites; o
teto padrão de bytes deriva da política de entradas e nunca supera 32 MiB.
`with_max_bytes` permite um teto menor. `memory_metrics` informa bytes atuais,
pico, máximo e rejeições por pressão sem expor os nonces.

Com protocolo V2 selecionado explicitamente, `PeerRpcChunkEncoder` lê um chunk
limitado de uma source `Read` e emite frames open/chunk/commit;
`PeerRpcChunkAssembler` verifica e escreve um chunk decodificado em sink
`Write`. O limite agregado default é 64 MiB. Input ausente, duplicado, fora de
ordem, corrompido, expandido acima da quota, expirado ou cancelado fecha o
assembler permanentemente. Finish com falha descarta o sink sem expor bytes
parciais como commitados. Em chunk identity, encoder e assembler movem a mesma
alocação owned em vez de clonar bytes decodificados em cada boundary. Uma sonda
fixa em stack sobre todo o chunk evita gzip especulativo apenas quando ele
parece já incompressível; chunks estruturados compressíveis ainda usam gzip.

`PeerRpcStreamRegistry` controla sessões V2 parciais com quotas explícitas de
sessões e bytes decodificados. Requisições usam arquivos exclusivos em um
diretório de spool existente e acessível somente pelo proprietário; apenas
payloads totalmente verificados chegam ao dispatcher e respostas usam pulls
explícitos e limitados. Erro, cancelamento, expiração e conclusão removem o
arquivo e a reserva. O snapshot informa sessões, bytes reservados, saturações e
limpezas.
Unix valida o proprietário efetivo e modos `0700`/`0600` do diretório/arquivo.
Windows rejeita reparse points e todo allow ACE fora do SID proprietário do
processo atual. Outras plataformas falham fechadas ao construir o registry.

Instale HTTP V2 explicitamente com
`PeerRpcHttpHost::with_v2_stream_registry`. O host default continua V1-only e
V2 usa JSON canônico por default. Framing binário exige o opt-in separado
`with_v2_binary_codec` no host e
`with_stream_codec_v2(PeerRpcStreamCodecV2::Binary)` no client. Ele usa paths
query/command distintos e o media type exato
`application/vnd.appcore.peer-rpc.v2+postcard`. Cada body selecionado exato é
autenticado e request/response avança um frame por vez.
O JSON canônico é serializado diretamente no SHA-256 da vinculação do token,
sem reter um segundo body codificado completo ao lado do frame.
Dependentes reutilizam esse caminho byte-exato por `json_payload_hash`.
Bodies binários nunca recebem gzip HTTP e permanecem abaixo de 256 KiB; gzip
opcional do chunk ainda é decodificado sob o limite declarado. Rota ausente,
mismatch de media type ou
reply malformado é terminal, sem fallback JSON. A admissão do open valida tenant, cluster,
target, trace, deadline, idempotência de command e nonce replay. Frames nunca
são repetidos após falha ambígua de transporte; cancelamento é best effort e a
limpeza por deadline é autoritativa.

Bodies de rejeição V2 usam `PeerRpcWireErrorV2`. O client valida code, phase,
retryability, retry delay, correlation e mensagem controlada pelo protocolo
como uma única matriz antes de retornar
`PeerRpcStreamClientErrorV2::Remote`. Codes desconhecidos são observáveis,
porém terminais e redigidos. Rejeições V1 viram
`PeerRpcError::RemoteRejected` por igualdade exata; disponibilidade e
capacidade de replay são os únicos casos V1 remotos com retry. Nenhum caminho
interpreta substring.

Disponibilidade do codec V2 não é negociação. O caller deve selecionar módulo
e transporte V2 explicitamente. `/v1/peer/*` interpreta somente V1 e não faz
fallback automático.

**Maturidade:** V1 estável; transporte V2 pós-1.0 certificado em desenvolvimento.

[Evidência de certificação do stream V2 limitado](benchmarks/peer-rpc-v2-2026-08-26.pt.md)
