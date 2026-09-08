# appcore-transport

Testes locais:

```bash
cargo test -p appcore-transport
```

**Responsabilidade:** mecânica HTTP/TLS compartilhada e limitada.

**Dependências internas:** nenhuma.

O crate possui SemVer independente. Adapters de infraestrutura podem consumi-lo
sem o host do AppCore Runtime.

**API principal:** `HttpScheme`, `HttpTarget`, `HttpRequest`, `HttpHeader`,
`HttpClient`, `HttpExchangeConfig`, `HttpTimeouts`, `HttpPoolConfig`,
`HttpResponse`, `CancellationToken`, `TransportError`, `send`, parse de
resposta e gzip limitado.

Mantenha e clone um `HttpClient` para reutilizar conexões HTTP/1.1 totalmente
consumidas. `HttpPoolConfig` limita conexões ativas, conexões ociosas e origens
retidas. `HttpTimeouts` separa os deadlines de conexão/admissão, leitura e
escrita. Respostas truncadas, malformadas ou com `Connection: close` nunca
voltam ao pool. A função `send` existente permanece um adapter V1 one-shot e
continua enviando `Connection: close`.

Bodies de request são bytes imutáveis compartilhados. `HttpRequest::new`
preserva o contrato de entrada `Vec` owned e o move para storage compartilhado;
`HttpRequest::from_shared_body` aceita um `Arc<[u8]>` existente. Clonar um
request ou transferi-lo para um worker limitado não duplica, portanto, um body
grande.

Use em adapters de infraestrutura que compartilham limites, timeout,
cancelamento e TLS. O consumidor mantém autenticação e policy. Não transforme
em framework web nem adicione endpoints de negócio.

O `Debug` de request/response mostra o tamanho do body, nunca seus bytes.
Headers conhecidos de credencial sao redigidos mesmo quando o chamador usa o
construtor de header nao sensivel.

**Maturidade:** superfície de infraestrutura RC estável.

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

`parse_response_owned` recebe ownership do frame completo. Ele compacta bodies
identity fixos e chunked dentro da mesma alocação e os retorna sem alocar uma
segunda cópia. `HttpClient` e o `send` one-shot usam esse caminho. Respostas
comprimidas preservam output limitado de decode; a API emprestada
`parse_response` não mudou.

## Documentação estável

ID estável: **ACR-004**. Consulte o
[guia complementar de arquitetura e integração](https://wiki.appcore.dnettoraw.com/pt/crates/id/acr-004). Esse ID permanente
continua válido se a página da wiki mudar.
