# appcore-transport

**Responsabilidade:** mecânica HTTP/TLS compartilhada e limitada.

**Dependências internas:** nenhuma.

O crate possui SemVer independente. Adapters de infraestrutura podem consumi-lo
sem o host do AppCore Runtime.

**API principal:** `HttpScheme`, `HttpTarget`, `HttpRequest`, `HttpHeader`,
`HttpClientConfig`, `HttpResponse`, `CancellationToken`, `TransportError`,
`send`, parse de resposta e gzip limitado.

Use em adapters de infraestrutura que compartilham limites, timeout,
cancelamento e TLS. O consumidor mantém autenticação e policy. Não transforme
em framework web nem adicione endpoints de negócio.

O `Debug` de request/response mostra o tamanho do body, nunca seus bytes.
Headers conhecidos de credencial sao redigidos mesmo quando o chamador usa o
construtor de header nao sensivel.

**Maturidade:** superfície de infraestrutura RC estável.
