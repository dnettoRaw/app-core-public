# appcore-peer-rpc

A entrada de corpos V1, JSON V2 e binário V2 compartilha 16 slots por host,
inclusive entre routers obtidos separadamente desse host. Saturação retorna
HTTP 503 antes da coleta; a recepção expira após 10 segundos (408). V1 mantém
o teto HTTP de 2 MiB; V2 mantém o teto de frame do registry. Health e manifest
não passam pela admissão de corpos. Essas falhas são de transporte antes do
decode, não respostas V2 assinadas. Corpos brutos admitidos ficam limitados a
16 vezes o maior teto habilitado, não ao RSS total. Decode, descompressão,
dispatch e respostas possuem limites separados.

Testes locais:

```bash
cargo test -p appcore-peer-rpc
```

**Responsabilidade:** client peer autenticado, host HTTP, validação e replay
protection.

**Dependências internas:** core, distributed contracts, security e transport.

**API principal:** traits de token issuer/authenticator/dispatcher e
implementações HashToken/static; nonce stores memória/arquivo; config,
validator e hashes; retry/client config e transport trait; transportes pooled e
standard one-shot; HTTP state e host.

Use `PooledPeerRpcTransport` para reutilizar conexões limitadas por origem.
`StdPeerRpcTransport` preserva o transporte V1 one-shot.
Ambos os transportes tomam ownership da alocação do body do DTO HTTP. Corpos V1
sem compressão e V2 exatos são movidos para `HttpRequest` sem clone integral;
V1 cria um buffer gzip separado somente quando ele é selecionado e menor.
O client V1 move cada payload outbound owned para um único envelope e reutiliza
esse owner nos retries limitados. Cada retry ainda renova timestamp, expiry,
nonce, vínculo de assinatura e body HTTP codificado. Com payload raw de 4 MiB,
cinco processos release no Apple M1 mantiveram p50 em +0,08%, reduziram RSS pico
em 7,21%, delta RSS da workload em 9,02% e delta retido em 7,99%.
Na entrada, `decode_peer_rpc_envelope_json` verifica o limite codificado e
desserializa um body V1 sem compressão diretamente dos bytes HTTP emprestados.
Depois a composition root move a alocação do payload decodificado para
`CommandEnvelope`; nenhuma fronteira retém um segundo payload completo.

Os nonce stores de memória e arquivo rejeitam por conta própria identifiers
acima de 128 bytes. O store de arquivo decodifica seu estado V1 owner-only por
um reader limitado a 16 MiB e serializa o mapa retido diretamente por um buffer
fixo de 64 KiB em temporário exclusivo. Corrupção completa, campos desconhecidos,
chaves inválidas e estado oversized falham fechados; escrita com falha remove o
stage. No Windows, a troca usa a operação atômica write-through da plataforma.

Dispatch V1 e V2 compartilha 16 permits blocking no processo. O host usa no
máximo 16 threads com stacks de 1 MiB e remove threads ociosas após cinco
segundos. Saturação falha antes da fila Tokio; V2 retorna `CapacityExceeded`.

O contrato opt-in `v2`, `PeerRpcChunkEncoder` e `PeerRpcChunkAssembler`
processam sources e sinks grandes usando um chunk limitado por vez. Os limites
default são 64 KiB decodificados por chunk, 96 KiB codificados, 64 MiB totais e
1.024 chunks. Sequência, tamanhos exatos, hash por chunk e total, deadline,
cancelamento e quota após descompressão falham fechados. Essas APIs de codec não
clonam chunks incompressíveis: a alocação owned passa da source para o frame e
depois para o receiver. Uma sonda fixa em stack sobre o chunk evita gzip
especulativo em dados provavelmente já comprimidos; chunks compressíveis ainda
usam gzip. Essas APIs não selecionam transporte V2 automaticamente; rotas V1
nunca inferem V2.

`PeerRpcStreamRegistry` adiciona quotas exatas de sessões e bytes decodificados,
spools exclusivos acessíveis somente pelo proprietário, pulls limitados para a
resposta do dispatcher e contadores de saturação/limpeza. Erro, cancelamento,
expiração e conclusão liberam o arquivo parcial e sua reserva.
Unix exige o proprietário efetivo com modos `0700`/`0600` no diretório/arquivo.
Windows rejeita reparse points e qualquer allow ACE fora do SID proprietário
do processo atual. Outras plataformas rejeitam a configuração do spool.

HTTP V2 é instalado somente por `PeerRpcHttpHost::with_v2_stream_registry`.
JSON continua sendo o codec default. O host chama também
`with_v2_binary_codec` e o client usa `with_stream_codec_v2(Binary)` para as
rotas Postcard separadas e bytes de chunk nativos. Cada body exato selecionado
é vinculado a um bearer token novo e processado incrementalmente. O JSON
canônico é serializado diretamente no SHA-256 dessa vinculação, sem reter um
segundo body codificado completo ao lado do frame. O helper público
`json_payload_hash` oferece o mesmo caminho byte-exato aos dependentes. Bodies
binários são limitados a 256 KiB e nunca comprimidos por HTTP; gzip limitado
por chunk continua dentro do frame assinado. Suporte binário ausente ou
incompatível é terminal e nunca faz fallback para JSON. O open reutiliza
validações de tenant, cluster, target, trace, deadline e nonce replay; commands
exigem idempotência. Frames não são repetidos após falha ambígua de transporte.
V1 continua sendo a superfície default e nunca faz upgrade automático.

[Evidência clean-source da certificação V2 de 64 MiB](wiki/benchmarks/peer-rpc-v2-2026-08-26.pt.md)

Use somente quando tenant, cluster, source, target, protocolo, expiry, nonce e
integridade podem ser provados. `AllowPeerAuthenticator` é somente teste.

O `Debug` dos DTOs peer request, response, outbound e HTTP mostra tamanhos e
omite bytes opacos, credenciais, valores de nonce/idempotencia e detalhes de
erro remoto.

`BoundedReplayStore` valida nonces no limite compartilhado de 128 bytes e
limita entradas vivas e bytes retidos estimados. Seu teto padrão derivado nunca
supera 32 MiB; `with_max_bytes` escolhe um teto menor e `memory_metrics` expõe
bytes atuais, pico, máximo e rejeições sem revelar os valores de nonce.

A matriz V2 também expõe resultados estáveis `capability_not_found`, `peer_busy`,
`timeout`, `stale`, `incompatible` e `transport_unavailable` para classificação
por operadores e UI. As mensagens pertencem ao protocolo e são redigidas.

`PeerRpcStreamRegistry::set_capability_limits` associa a uma capability limites
explícitos de bytes decodificados de request, bytes de response e timeout.
Capabilities sem registro mantêm somente os limites gerais do registry; não há
ampliação implícita de policy. Request ou response rejeitado é removido antes
da publicação.

`PeerRpcChunkTransferRequestV2` e `PeerRpcChunkTransferResponseV2`, independentes
de update, fornecem transferência limitada por hash do objeto, offset e tamanho.
`serve_chunk` lê somente o range solicitado de uma source seekable, e
`verify_chunk` valida identidade, metadados do range e digest antes que um sink
retomável faça commit.

**Maturidade:** V1 estável; transporte V2 pós-1.0 certificado em desenvolvimento.

## Documentação estável

ID estável: **ACR-017**. Consulte o
[guia complementar de arquitetura e integração](https://wiki.appcore.dnettoraw.com/pt/crates/id/acr-017). Esse ID permanente
continua válido se a página da wiki mudar.

[Migração de envelopes de aplicação](wiki/transport-migration.pt.md).

## Durabilidade do nonce e diagnóstico tipado

Esta secção descreve alterações ainda não publicadas do código-fonte. Verifique o pacote exato do registry antes da adoção pelos consumidores.

A publicação do nonce no Windows sincroniza o ficheiro e substitui com `MOVEFILE_WRITE_THROUGH`; não reabre o parent como diretório Unix. Unix conserva o sync do parent. `PeerRpcDiagnosticV2::from_wire` conserva código, fase, retry e correlação validados, identidade confiável do peer e ação definida pelo protocolo. Códigos desconhecidos continuam redigidos e terminais. Expiração sugere verificar deadline/relógio, sem provar desvio do relógio. Uma sugestão de retry não autoriza replay de command com ack ambíguo. É uma projeção local aditiva; os campos wire V1/V2 permanecem iguais.

## Vinculação explícita da operação no token V2 para envelopes V1

Perfil de autenticação aditivo ainda não publicado: selecione `with_v1_operation_binding_v2` tanto em `PeerRpcClient` como em `PeerRpcHttpHost`. `operation_signing_hash_v2` vincula domínio V2 explícito, `query`/`command` e digest completo do envelope V1 existente. Schema, número de protocolo e rotas V1 não mudam; o contrato de vinculação do token é explicitamente V2. Nenhum lado negoceia, deduz ou aceita digest alternativo. Perfis diferentes falham na autenticação. Health/manifest e streaming V2 mantêm a autenticação existente.

O perfil V1 publicado por omissão mantém apenas o envelope para estabilidade contratual e não vincula criptograficamente a classe da rota. Selecione o perfil com operação ou streaming V2 quando essa fronteira importar; o dispatcher deve sempre autorizar capability e modo de invocação. Atualize ambos os endpoints atomicamente ou transfira tráfego para um host configurado separadamente. Drene tentativas de commands ambíguas primeiro. O rollback exige restaurar explicitamente a seleção de ambos os lados; não repita um command com ACK incerto usando chave nova. Esta alteração não certifica nem publica uma migração da instalação.

## Candidato de release — 2026-10-14

`1.0.5-rc.1`: alterações próprias.
