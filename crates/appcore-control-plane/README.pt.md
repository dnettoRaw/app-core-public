# appcore-control-plane

Descartar a future antes do despacho impede a execução da operação. A fila
mantém referência fraca ao resultado, sem reter waker ou resposta de uma future
abandonada. A closure e o slot permanecem até sair da fila. Após o despacho,
descartar a future não desfaz efeitos remotos nem interrompe transporte síncrono;
use cancelamento cooperativo e reconcilie resultados ambíguos.

Aquisição/renovação e liberação de lease usam uma tentativa HTTP, independentemente
da configuração de retries. V1 não fornece chave de deduplicação remota; a
resposta pode se perder após aplicar a mutação. Timeout ou falha HTTP transitória
não provam que o lease ficou inalterado. Reconcilie estado autoritativo e fencing
antes de escritas dependentes de liderança; não repita a operação cegamente.
Discovery, registro e heartbeat mantêm retries configurados; a semântica remota
ainda exige testes de conformidade do deployment.

As esperas de retry usam jitter não criptográfico entre metade (arredondada
para cima) e o teto exponencial atual, limitadas também ao tempo restante.
Backoff zero continua zero. Cada ciclo inicializa o estado local com o hasher
aleatorizado da biblioteca padrão; isso não é aleatoriedade de segurança.

A configuração de retries HTTP é limitada ao executar o request: até 16
tentativas, timeout de 1–30.000 ms e backoff de até 30.000 ms. Zero tentativas
continua significando uma; backoff zero é permitido. O orçamento conservador é
tentativas × timeout + (tentativas − 1) × backoff máximo, limitado a 120 segundos.
Relógio monotônico limita cada tentativa e espera ao tempo restante. Uma resposta
tardia retorna Timeout sem nova tentativa, mesmo se indicar sucesso; a operação
remota pode já ter sido aplicada. Transportes externos devem respeitar o prazo:
callbacks síncronos não podem ser interrompidos à força. O orçamento começa
na chamada do provider e inclui fila, encode e decode. Requests vencidos na
fila não chegam ao transporte. O vencimento é observado quando o worker avança,
sem timer independente: um transporte anterior preso pode atrasar a future.

Testes locais:

```bash
cargo test -p appcore-control-plane
```

Retries HTTP se limitam às respostas 408, 429, 500, 502, 503 e 504 e a
falhas de transporte, timeout ou offline. Outros status e falhas semânticas
tipadas retornam imediatamente. O primeiro backoff também respeita
`max_backoff_ms`. O prazo local total descrito acima não prova que uma escrita
remota de resultado ambíguo pode ser repetida com segurança.

**Responsabilidade:** implementações genéricas de presença, heartbeat, discovery
e leases.

**Dependências internas:** contracts, core, distributed contracts e transport.

**API principal:** clients in-memory, file e offline; configuração HTTP, retry
policy e transport trait; transports one-shot standard, pooled e bearer;
coordinator e heartbeat policy; guards de liderança global/serviço; validação
de endpoint seguro.

`PooledHttpTransport` é o perfil HTTP reutilizável sem autenticação e
`BearerHttpTransport` também reutiliza seu cliente limitado.
`StdHttpTransport` preserva o perfil V1 one-shot.
`HttpControlPlaneClient` converte cada body codificado uma vez em
`SharedHttpControlPlaneRequest` e o empresta entre retries limitados. Os
transports internos reutilizam a mesma alocação imutável; transports externos
que implementam apenas o método owned original mantêm o fallback compatível.
O `Debug` do request informa o tamanho do body sem expor seus bytes.

Use para coordenação distribuída sem payload de negócio. Perfil file exige
locks/storage certificados. Perfil remoto exige TLS e autenticação do
deployment.

O perfil file limita estado e backup a 16 MiB e rejeita estado malformado ou
futuro. A aritmética de expiração e epoch é verificada; o esgotamento do epoch
falha fechado em vez de reutilizar um fencing token.

O perfil em memória admite por padrão no máximo 65.536 registros e slots de
lease combinados sob um orçamento estimado de 16 MiB retidos. Use `with_limits`
para reduzir ambos os limites e `stats` para observar bytes atuais/de pico e
rejeições. Uma substituição grande demais preserva o registro ou lease anterior.
O perfil em arquivo também limita o estado decodificado a 262.144 registros e
64 MiB, mantendo inalterado o limite V1 de 16 MiB do JSON.

O JSON de estado é decodificado por reader limitado e serializado diretamente
em arquivo temporário exclusivo. Backup e restore transmitem um único arquivo
limitado para uma geração staged, validam essa geração exata, sincronizam e só
então substituem o destino. Os mapas decodificados são o único owner completo
do estado; não existe um segundo buffer JSON completo.

**Maturidade:** contratos e referências RC estáveis; operação do serviço
externo pertence ao deployment.

## Documentação estável

ID estável: **ACR-015**. Consulte o
[guia complementar de arquitetura e integração](https://wiki.appcore.dnettoraw.com/pt/crates/id/acr-015). Esse ID permanente
continua válido se a página da wiki mudar.
