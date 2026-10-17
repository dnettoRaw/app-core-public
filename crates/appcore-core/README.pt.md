# appcore-core

Release candidate **`appcore-core 1.0.4-rc.2`**. Selecionar esta RC explicitamente; não substitui versões estáveis anteriores nem certifica um deployment. Quando existem, as dependências AppCore internas estão fixadas nesta rodada RC.

O benchmark runtime compara a redaction atual com o algoritmo preservado de
`efe9a205`: `redaction_plain_8192_{current,reference}` e
`redaction_mixed_{current,reference}`. Criação das fixtures e igualdade das saídas
ficam fora do tempo; cada iteração inclui redaction e liberação do resultado.
As implementações usam o mesmo binário otimizado, com amostras em processos
separados. Esses casos medem texto diagnóstico, não I/O do journal inteiro nem
contagem de alocações. A referência existe somente no benchmark.

A redaction verifica se o texto completo já está seguro e limitado antes das
passagens por marcadores; esse caminho aloca somente o resultado owned. Nos
demais casos, mantém a busca anterior sobre cópia em minúsculas, mas reutiliza
a saída quando o marcador está ausente. Variantes de busca direta foram
rejeitadas porque os benchmarks pareados mostraram regressões no texto misto.
As sete passagens preservam ordem, delimitadores e corte
UTF-8 anteriores. O resultado público continua sendo String owned; passagens
com matches podem alocar. Não há alegação de ganho de throughput ou RSS sem
benchmark pareado. Continua sendo redaction conservadora por marcadores, não
um parser capaz de identificar todo segredo em payloads arbitrários.

Testes locais:

```bash
cargo test -p appcore-core
```

Registries e engines de decisão admitem até 4.096 nomes únicos de 1–256
bytes UTF-8. Registros inválidos, duplicados ou excessivos falham antes da
retenção, preservando a ordem existente. Isso limita metadata do registry,
não a memória interna de nós fornecidos pela aplicação.

**Responsabilidade:** lifecycle, registro, dispatch, state, audit e idempotência
genéricos dentro do processo.

**Dependências internas:** `appcore-contracts`, `appcore-types`.

**API principal:** `RuntimeBuilder`, `RuntimeController`, `RuntimeInstance`,
`RuntimeLifecycle`, registries e buses de command/event, envelopes,
`CommandHandler`, `CommandResult`, `RuntimeContext`, audit log/journal,
idempotência em memória/arquivo, state e decision engines, clock, redaction e
`AppPlugin` de compatibilidade.

Clones de `RuntimeController` compartilham lifecycle, idempotência e comandos
em execução. O command bus imutável possui handlers por `Arc`. Handlers
independentes podem executar em paralelo, enquanto uma mesma chave idempotente
admite no máximo uma execução. O shutdown fecha a admissão atomicamente e
permite drenagem limitada dos comandos já admitidos.

`RuntimeLifecycle` guarda um único enum de estado `Copy` no mutex e aplica as
12 transições estáveis exatas por uma função total. Nenhum nome validado ou
tabela de transições é alocado por instância. A `StateMachine` pública genérica
continua disponível e inalterada para estados pertencentes à aplicação.

A idempotência em arquivo é percorrida incrementalmente. O journal V1 limita o
arquivo a 64 MiB, cada registro a 1 MiB, o intervalo entre compactações a
131.072 registros persistidos e o estado ativo a 65.536 chaves. O append
compacta atomicamente antes de cruzar um limite; o startup descarta somente um
registro final incompleto e limitado, sem aceitar registros completos
corrompidos. O store mantém apenas chaves e offsets verificados, carregando um
corpo de resposta sob demanda em vez de reter todos os replays no heap. A
idempotência em memória usa o mesmo teto de chaves ativas.

`FileOperationalJournal` também percorre uma linha limitada por vez e rejeita
registros acima de 1 MiB. Hash, append e compactação atômica serializam
diretamente para contadores, digests e arquivos, sem buffers JSON completos nem
clones dos registros retidos. `write_audit_jsonl` transmite o export para um
writer do caller; `export_audit_jsonl` permanece como adaptador compatível que
retorna uma `String` owned.

O `AuditLog` local ao processo tem orçamento agregado padrão de 16 MiB entre
seus snapshots de commands e entradas genéricas, além dos tetos de 10.000
itens. `with_max_bytes` pode apertar o limite, `stats` expõe bytes atuais/de
pico, evictions e rejections, e `write_jsonl` serializa um snapshot
copy-on-write compartilhado no writer do caller sem manter o lock durante I/O.
Clonar o log compartilha snapshots imutáveis até uma mutação.

Use `entries_snapshot` quando um export estruturado e limitado precisar de um
array JSON. Ele captura a fila imutável compartilhada sem clonar os campos das
entradas e implementa `Serialize`; mutações posteriores não alteram a visão.
O benchmark pretty-JSON de 10.000 entradas e 2.996.676 bytes mediu 1,12 ms p50
e 6,42 MiB de RSS pico no Apple M1.

`records_snapshot` oferece o mesmo contrato para registros de command. Os dois
tipos de snapshot expõem `recent(limit)` para uma query limitada emprestar
somente sua página mais nova depois de liberar o lock. Um tail de 1.000 itens
sobre 10.000 registros e entradas mediu 2,06 us p50 e 11,88 MiB de RSS pico,
contra 4,16 ms e 20,33 MiB dos métodos compatíveis de cópia integral.

Com um `FileOperationalJournal` anexado, novas entradas de audit e entradas
seguras restauradas compartilham com o journal uma única alocação imutável do
registro operacional. A leitura do journal valida a hash chain, verifica o
texto de audit sem alocar, sanitiza apenas conteúdo inseguro e o regrava
atomicamente antes de expô-lo. Anexar o log depois disso copia somente handles
`Arc` limitados. Accessors owned públicos, JSON do snapshot e formato V1 do
journal não mudam. Uma anexação sobre 384 entradas seguras (cerca de 3 MiB)
reduziu p50 de 12,26 ms para 86,50 us (-99,29%), RSS pico em 0,57% e RSS da
carga em 1,72% no Apple M1. A carga separada com fsync conserva os ganhos
anteriores de 27,83% em p50, 37,30% em RSS pico e 47,93% em memória retida.

O `EventBus` local ao processo também retém no máximo 10.000 eventos e 16 MiB
por padrão. `with_max_bytes` aperta o teto, `stats` informa pressão por bytes,
evictions e rejeições de eventos grandes, e `snapshot().recent` empresta uma
página mais nova estável. Selecionar 1.000 de 10.000 eventos mediu 2,39 us p50
e 8,48 MiB de RSS pico, contra 2,09 ms e 14,59 MiB para `events()`.
Quando um `FileOperationalJournal` está anexado, bus e journal retêm a mesma
alocação imutável do registro de evento. O restore copia apenas handles `Arc`
limitados; as APIs públicas owned e o formato V1 do journal não mudam. Uma
carga com 3 MiB de eventos retidos reduziu o RSS pico de 8,11 para 5,08 MiB
(-37,38%) e a memória retida da carga em 48,00%, com p50 dominado por disco
dentro de 0,95%.

Aplicações novas usam `appcore_sdk::Application`; não montam o core
manualmente. Mantenha I/O adapters e comportamento de domínio fora.

**Maturidade:** superfície low-level RC estável; builder/plugin são de
compatibilidade e manifest-first é o caminho preferido.

## Documentação estável

ID estável: **ACR-008**. Consulte o
[guia complementar de arquitetura e integração](https://wiki.appcore.dnettoraw.com/pt/crates/id/acr-008). Esse ID permanente
continua válido se a página da wiki mudar.

## Identidade da operação e recuperação da conclusão

Contrato de código-fonte ainda não publicado: a chave é única no store; `command-v2:` vincula ID da aplicação, nome do command e payload delimitado por comprimento com SHA-256. Node, ID do pedido, data e trace são metadados de transporte e podem mudar na repetição. Outra aplicação ou operação com a mesma chave devolve `IdempotencyConflict`. Fingerprints apenas do payload falham no dispatch com `NO MORE SUPPORTED PLEASE UPDATE`, sem conversão. Drene e reconcilie a instalação antiga, arquive o store e selecione explicitamente um store novo antes de atualizar. O rollback usa executável e store originais; nunca os misture.

São admitidas no máximo 64 conclusões com chave, em execução ou recuperáveis, com resultado serializado de até 1 MiB cada (limite de 64 MiB de corpos retidos). Uma falha na escrita do recibo retém o resultado no buffer partilhado do controller. A repetição idêntica persiste o mesmo recibo e publica eventos uma vez neste processo, sem repetir o handler, incluindo uma escrita ambígua já realizada pelo store. `recoverable_command_completions` conta resultados retidos. O buffer cheio rejeita antes do handler. Um resultado excessivo deixa uma reserva durável pendente para reconciliação.

A abertura do ficheiro e a compactação TTL preservam `Pending`; só recibos resolvidos expiram. Após perda do processo, o dispatch não deduz o efeito externo. A aplicação/deployment confiável reconcilia evidência autoritativa e chama `recover_pending_command` com a operação original e o resultado limitado. Reservas ativas, resolvidas, ausentes ou divergentes são rejeitadas. Falhas normais do handler libertam a reserva quando a remoção no store tem sucesso.

Efeito da aplicação, persistência do recibo e publicação de eventos são fronteiras separadas. Um crash entre recibo e publicação ainda pode perder a publicação. O EventBus limitado e o journal operacional opcional não são uma outbox transacional da aplicação. Para recuperação durável, grave efeito, resultado autoritativo e IDs estáveis dos eventos na transação/outbox da aplicação e deduplique consumidores. Não apague uma reserva pendente nem crie uma chave nova para contornar a incerteza.

## Candidato de release — 2026-10-14

`1.0.4-rc.1`: alterações próprias.
