# appcore-transport

[Exemplo minimo](examples/basic.pt.md) |
[Exemplo intermediario](examples/intermediate.pt.md)

**Responsabilidade:** mecânica HTTP/TLS compartilhada e limitada.

**Dependências internas:** nenhuma.

**Versionamento:** SemVer independente. O crate pode ser consumido sem qualquer
outro pacote AppCore.

**API principal:** `HttpScheme`, `HttpTarget`, `HttpRequest`, `HttpHeader`,
`HttpClient`, `HttpExchangeConfig`, `HttpTimeouts`, `HttpPoolConfig`,
`HttpClientConfig`, `HttpResponse`, `CancellationToken`, `TransportError`,
`send`, parse de resposta e gzip limitado.

Um `HttpClient` possui um pool limitado por scheme, host e porta. Seus clones
compartilham o mesmo pool. A admissão é limitada por origem, a espera respeita o
deadline de conexão e o cancelamento, e tanto origens quanto sockets ociosos
são limitados e expiram. Somente uma resposta completamente delimitada e
interpretada permite reutilizar o socket. Truncamento, framing inválido,
timeout, cancelamento, `Connection: close` e body delimitado por fechamento
descartam o socket.

Use `HttpExchangeConfig` e `HttpTimeouts` para deadlines independentes de
conexão/admissão, leitura e escrita. `HttpClientConfig` e a função livre `send`
preservam o contrato V1 one-shot, incluindo `Connection: close`; consumidores
existentes não entram em pooling silenciosamente.

`HttpRequest` guarda o body como bytes imutáveis compartilhados. O construtor
compatível `new` move um `Vec<u8>` owned para esse storage, enquanto
`from_shared_body` reutiliza um `Arc<[u8]>` do caller. Clones do request
compartilham a mesma alocação; isso atende workers de transporte limitados que
precisam possuir o request depois que a future chamadora cede execução.

Use em adapters de infraestrutura que compartilham limites, timeout,
cancelamento e TLS. O consumidor mantém autenticação e policy. Não transforme
em framework web nem adicione endpoints de negócio.

O `Debug` de request/response mostra o tamanho do body, nunca seus bytes.
Headers conhecidos de credencial sao redigidos mesmo quando o chamador usa o
construtor de header nao sensivel.

`encode_gzip_if_smaller` deixa de reter o candidato gzip quando os bytes
produzidos alcançariam o tamanho da entrada e retorna `None`. Pedidos de
crescimento do buffer não excedem `input.len() - 1`; entrada vazia retorna
`None` sem criar o codec. Isso não limita memória interna do codec, overhead
do allocator, entrada ou CPU gasta antes da emissão de bytes. Candidatos úteis
preservam os mesmos bytes gzip; nenhuma heurística descarta entrada promissora.

Em respostas gzip sem chunking, o parser toma os bytes comprimidos emprestados
da entrada durante o decode, evitando uma segunda alocação do corpo comprimido.
O body retornado continua owned. Gzip chunked primeiro é compactado na própria
alocação, mas ainda exige output descomprimido; isso não é streaming.

Quando o frame completo já é owned, `parse_response_owned` reutiliza essa
alocação para bodies identity fixos e chunked. Os clients integrados usam esse
caminho; callers emprestados mantêm `parse_response`. Decode comprimido
continua limitado, mas ainda exige output owned.

**Maturidade:** superfície de infraestrutura RC estável.
