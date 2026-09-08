# appcore-ops

Testes locais:

```bash
cargo test -p appcore-ops
```

**Responsabilidade:** health, logs, metrics, observations, heartbeat e
availability sem dependência de vendor.

**Dependências internas:** `appcore-core`, `appcore-supervisor`.

**API principal:** health status/report/checks, heartbeat sources, loggers,
metric counters, `ObservationEvent`/`ObservationSink`, file sink limitado,
availability report e reexports de compatibilidade para
`appcore-supervisor::managed_services`.

`InMemoryObservationSink` limita entradas e bytes retidos estimados, nunca
pré-aloca a partir de uma capacidade não confiável e oferece views imutáveis
`ObservationSnapshot`. `InMemoryMetrics` também limita tamanho do nome,
cardinalidade e bytes; nomes rejeitados aparecem nos contadores de pressão. Os
métodos `snapshot` compatíveis ainda retornam valores owned, enquanto
`shared_snapshot` evita clonar nomes e eventos retidos.
`InMemoryLogger` aplica o mesmo padrão a 4.096 registros e 8 MiB; sua view
`shared_records` evita clonar texto de log redigido.
A configuração de drains é uma geração imutável copy-on-write. Cada observação
compartilha essa geração com um único clone de `Arc`, em vez de clonar até 32
handles de drain, e todo callback continua executando após liberar o lock de
configuração.
`SharedObservationEvent::new` aplica redaction e limites uma única vez. Os sinks
de memória, arquivo e métricas sobrescrevem `ObservationSink::emit_shared` para
que todos os drains retenham ou inspecionem um único payload imutável; a
implementação default preserva sinks existentes que aceitam apenas ownership.
Chaves de atributos sensíveis são verificadas por uma varredura de bytes ASCII
case-insensitive sem alocação. A política conservadora existente de substrings
permanece igual, sem alocar uma `String` em minúsculas para cada atributo.

O worker de observações em arquivo revalida os campos públicos do evento antes
da admissão na fila limitada. Cada registro JSONL é contado por um writer
limitado e depois serializado diretamente no arquivo ativo, sem reter um segundo
buffer JSON completo. Um registro que não cabe ao lado do header V1 em um
arquivo vazio é rejeitado e incrementa `FileObservationSinkStats::errors`; ele
nunca cria uma rotação acima do limite. A admissão também limita a fila a
65.536 itens e 8 MiB entre registros enfileirados e em escrita.
`FileObservationSink::pressure` informa bytes atuais, pico e rejeições pelo
orçamento de bytes.
`flush` aplica `FILE_OBSERVATION_FLUSH_TIMEOUT`, de 30 segundos, tanto à
admissão na fila limitada quanto ao acknowledgement do worker. Use
`flush_timeout` para um prazo operacional positivo menor; saturação ou worker
travado retorna `TimedOut`. Um flush já admitido pode terminar depois do
timeout do caller, sem duplicar nem cancelar à força o I/O do filesystem.

Use para sinais operacionais genéricos. Código novo de lifecycle usa
`appcore-supervisor` diretamente. Não adicione SDK de vendor nem métricas de
negócio da aplicação ao crate.

**Maturidade:** primitives RC estáveis; export/collection de produção pertence
ao deployment.

## Retenção de snapshots de métricas

Um snapshot compartilhado é imutável, mas mantê-lo durante uma atualização faz
essa atualização copiar os nós do mapa. Os nomes continuam compartilhados.
A pressão do registry cobre somente a geração atual, não todos os snapshots
retidos pelos consumidores. Limitar nomes não limita a memória do processo.

O coletor deve descartar o snapshot anterior antes de obter o próximo, ou usar
uma fila explicitamente limitada que remove antes de admitir. Limite todos os
consumidores e clones, incluindo exports em andamento. Não substitua valores
do snapshot por leituras atômicas vivas: isso mudaria a semântica temporal.

A família de benches `metric_update_4096_retained_0/1/16` intercala snapshots
e updates de 4.096 contadores com 0, 1 ou 16 gerações retidas. Fixture e
preenchimento inicial da retenção ficam fora do timer; aquisição, remoção e
update são medidos. RSS inclui gerações aquecidas; o checkpoint de retenção
ocorre após liberá-las e pode incluir memória retida pelo allocator. Esses
casos não certificam o orçamento agregado de um deployment.

## Documentação estável

ID estável: **ACR-013**. Consulte o
[guia complementar de arquitetura e integração](https://wiki.appcore.dnettoraw.com/pt/crates/id/acr-013). Esse ID permanente
continua válido se a página da wiki mudar.
