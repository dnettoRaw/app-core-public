# appcore-core

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

Registries e engines de decisão admitem até 4.096 nomes únicos de 1–256
bytes UTF-8. Registros inválidos, duplicados ou excessivos falham antes da
retenção, preservando a ordem existente. Isso limita metadata do registry,
não a memória interna de nós fornecidos pela aplicação.

[Exemplo minimo](examples/basic.pt.md) |
[Exemplo intermediario](examples/intermediate.pt.md)

**Responsabilidade:** lifecycle, registro, dispatch, state, audit e idempotência
genéricos dentro do processo.

**Dependências internas:** `appcore-contracts`, `appcore-types`.

**API principal:** `RuntimeBuilder`, `RuntimeController`, `RuntimeInstance`,
`RuntimeLifecycle`, registries e buses de command/event, envelopes,
`CommandHandler`, `CommandResult`, `RuntimeContext`, audit log/journal,
idempotência em memória/arquivo, state e decision engines, clock, redaction e
`AppPlugin` de compatibilidade.

Valores clonados de `RuntimeController` compartilham lifecycle, idempotência e
comandos em execução. O command bus imutável possui handlers por `Arc`.
Handlers independentes podem executar em paralelo, enquanto uma chave
idempotente admite no máximo uma execução. Solicite o shutdown antes da
drenagem limitada; novos comandos são rejeitados sem corrida com a transição de
lifecycle.

`RuntimeLifecycle` guarda um único enum de estado `Copy` no mutex e aplica as
12 transições estáveis exatas por uma função total. Nenhum nome validado ou
tabela de transições é alocado por instância. A `StateMachine` pública genérica
continua disponível e inalterada para estados pertencentes à aplicação.

`FileIdempotencyStore` percorre o journal V1 uma linha limitada por vez, sem
materializar o arquivo. Os limites de arquivo, registro e chaves ativas são
64 MiB, 1 MiB e 65.536; no máximo 131.072 registros de journal permanecem entre
compactações atômicas. Uma linha final incompleta e limitada é recuperada, mas
linhas completas inválidas falham fechadas. O mapa residente contém chaves e
offsets verificados por SHA-256, não corpos de resposta; `get` lê somente o
registro limitado selecionado. `InMemoryIdempotencyStore` usa o mesmo teto de
chaves ativas para manter um limite explícito de memória.

`FileOperationalJournal` aplica a mesma disciplina incremental à persistência
de audit e eventos. No startup, no máximo uma linha de registro de 1 MiB fica
em memória durante a validação da hash chain. O append calcula o hash por um
contador limitado e um digest writer, e a compactação encontra o maior sufixo
que cabe antes de transmitir uma única substituição atômica. Use
`write_audit_jsonl` com um sink owned pelo caller; o `export_audit_jsonl` existente
materializa intencionalmente a `String` solicitada.

O `AuditLog` em memória limita separadamente seus dois snapshots a 10.000 itens
e usa um orçamento padrão compartilhado de 16 MiB. Use `with_max_bytes` para
apertá-lo, consulte `stats` para pressão atual/de pico, evictions e rejections,
e prefira `write_jsonl` para transmitir um snapshot copy-on-write após liberar
o lock de estado. `export_jsonl` continua o adaptador compatível owned.

Para um array JSON estruturado, chame `entries_snapshot`. A visão imutável
retornada implementa `Serialize`, compartilha as entradas em vez de cloná-las
profundamente e permanece estável se o log ativo mudar. A carga pretty-JSON de
10.000 entradas e 2.996.676 bytes mediu 1,12 ms p50 e 6,42 MiB de RSS pico no
Apple M1.

`records_snapshot` é a visão correspondente dos registros de command. Ambos os
snapshots oferecem `recent(limit)` para uma página mais nova emprestada depois
que o lock é liberado. Selecionar 1.000 de 10.000 registros e entradas mediu
2,06 us p50 e 11,88 MiB de RSS pico, contra 4,16 ms e 20,33 MiB das cópias
owned integrais.

Quando `AuditLog` está anexado ao `FileOperationalJournal`, entradas novas e
entradas seguras restauradas retêm o mesmo `Arc<OperationalJournalRecord>`
imutável. A leitura do journal primeiro valida a hash chain e então usa uma
checagem de texto sem alocações. Conteúdo inseguro é limitado, redigido e
regravado atomicamente uma única vez; anexações posteriores copiam apenas
handles `Arc` limitados. Accessors owned públicos, serialização do snapshot e
encoding V1 em disco não mudam. Uma anexação sobre 384 entradas seguras (cerca
de 3 MiB) reduziu p50 de 12,26 ms para 86,50 us (-99,29%), RSS pico em 0,57% e
RSS da carga em 1,72% no Apple M1. A carga pareada com fsync conserva os ganhos
anteriores de 27,83% em p50, 37,30% em RSS pico e 47,93% em memória retida.

O `EventBus` local ao processo tem o mesmo formato explícito de memória: no
máximo 10.000 eventos e orçamento padrão compartilhado de 16 MiB. Use
`with_max_bytes` para apertá-lo; `stats` expõe bytes atuais/de pico, evictions e
rejections; `snapshot().recent(limit)` seleciona uma página estável sem clonar
payloads. A seleção de 1.000 em 10.000 mediu 2,39 us p50 e 8,48 MiB de RSS pico,
contra 2,09 ms e 14,59 MiB do adaptador compatível de cópia integral.
Com um `FileOperationalJournal` anexado, os dois owners retêm um único
`Arc<OperationalJournalRecord>` imutável por evento, não duas alocações do
payload. O restore copia apenas handles `Arc` limitados. Accessors owned
públicos, serialização do snapshot e registro V1 em disco permanecem iguais.
Uma carga com 3 MiB de eventos retidos reduziu o RSS pico de 8,11 para 5,08 MiB
(-37,38%) e a memória retida em 48,00%; o p50 dominado por disco mudou +0,95%.

Aplicações novas usam `appcore_sdk::Application`; não montam o core
manualmente. Mantenha I/O adapters e comportamento de domínio fora.

**Maturidade:** superfície low-level RC estável; builder/plugin são de
compatibilidade e manifest-first é o caminho preferido.
