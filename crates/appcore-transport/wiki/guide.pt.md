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

Use em adapters de infraestrutura que compartilham limites, timeout,
cancelamento e TLS. O consumidor mantém autenticação e policy. Não transforme
em framework web nem adicione endpoints de negócio.

O `Debug` de request/response mostra o tamanho do body, nunca seus bytes.
Headers conhecidos de credencial sao redigidos mesmo quando o chamador usa o
construtor de header nao sensivel.

**Maturidade:** superfície de infraestrutura RC estável.
