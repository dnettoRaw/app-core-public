# appcore-ops

[Exemplo minimo](examples/basic.pt.md) |
[Exemplo intermediario](examples/intermediate.pt.md)

**Responsabilidade:** health, logs, metrics, observations, heartbeat e
availability sem dependência de vendor.

**Dependências internas:** `appcore-core`, `appcore-supervisor`.

**API principal:** health status/report/checks, heartbeat sources, loggers,
metric counters, `ObservationEvent`/`ObservationSink`, file sink limitado,
availability report e reexports de compatibilidade para
`appcore-supervisor::managed_services`.

O sink de observações local ao processo retém no máximo 65.536 eventos e 16
MiB, com teto de bytes menor derivado de políticas de quantidade menores. O
registro de métricas retém no máximo 4.096 nomes, 128 bytes por nome e 1 MiB
agregado. Ambos expõem pressão de quantidade/bytes e snapshots imutáveis
compartilhados; snapshots compatíveis ainda geram valores owned. Observações
grandes demais não são retidas, mas continuam chegando aos no máximo 32 drains
configurados. O logger em memória também retém no máximo 4.096 registros e 8
MiB e oferece `shared_records`.
A configuração de drains usa uma geração imutável copy-on-write. O `emit`
compartilha essa geração com um clone de `Arc`, em vez de clonar até 32 handles,
libera o lock de configuração e somente então chama os drains.
`SharedObservationEvent::new` aplica redaction e limites de campos uma única
vez. O hub em memória encaminha esse payload imutável por
`ObservationSink::emit_shared`; os sinks de memória, arquivo e métricas
sobrescrevem o método sem clone profundo. Implementações existentes continuam
precisando somente de `emit` e recebem o fallback owned automaticamente.
Nomes de atributos sensíveis usam uma varredura de bytes ASCII case-insensitive
sem alocação. O matching conservador por substring permanece igual, mas nenhuma
`String` em minúsculas é criada para cada atributo.

O file sink limitado valida novamente nome, trace, quantidade de atributos,
chaves e valores no `emit`, inclusive para eventos montados pelos campos
públicos. O worker mede um registro JSONL com um counting writer limitado antes
da rotação e transmite o mesmo registro para o disco. Ele nunca aloca uma cópia
serializada completa. Um registro maior que o espaço útil de um arquivo vazio
falha fechado e aparece em `FileObservationSinkStats::errors`, sem rotacionar.
A fila aceita no máximo 65.536 itens e retém no máximo 8 MiB entre registros
enfileirados e em escrita. `FileObservationSink::pressure` expõe bytes atuais,
pico e rejeições pelo orçamento de bytes.

`FileObservationSink::flush` usa `FILE_OBSERVATION_FLUSH_TIMEOUT`, de 30
segundos. Seu deadline único começa antes da admissão na fila limitada e inclui
o acknowledgement durável do worker. Chame
`flush_timeout(Duration::from_secs(...))` para prazo positivo menor. Fila cheia
ou ausência de acknowledgement retorna `ErrorKind::TimedOut`; duração zero ou
com overflow retorna `InvalidInput`. Se o comando já entrou na fila, o worker
pode terminá-lo com segurança depois do timeout do caller.

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
