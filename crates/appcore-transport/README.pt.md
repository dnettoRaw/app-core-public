# appcore-transport

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

Use em adapters de infraestrutura que compartilham limites, timeout,
cancelamento e TLS. O consumidor mantém autenticação e policy. Não transforme
em framework web nem adicione endpoints de negócio.

O `Debug` de request/response mostra o tamanho do body, nunca seus bytes.
Headers conhecidos de credencial sao redigidos mesmo quando o chamador usa o
construtor de header nao sensivel.

**Maturidade:** superfície de infraestrutura RC estável.
