# appcore-sync-sqlite

[English](guide.en.md) | [Français](guide.fr.md) |
[Básico](examples/basic.pt.md) | [Intermediário](examples/intermediate.pt.md)

**Camada:** integration. **Status:** prerelease opcional. Versão no workspace
`0.1.0-alpha.4`, revisada em 2026-09-05; esta revisão do código não atesta
publicação no registry nem certificação para produção.

`SqliteSyncStore::open` resolve o path para um local estável, rejeita target de
database por symlink, configura WAL e limites do SQLite, executa somente
migrations transacionais conhecidas e verifica integridade antes de retornar.
Corrupção completa e formatos desconhecidos falham fechados com erros redigidos.

O schema interno V2 entrega ao `SqliteSyncOutbox` paginação limitada, stats sem
payload, metadata durável de attempt/readiness e receipts parciais ordenados e
transacionais. A metadata da página é selecionada antes de materializar BLOBs.
Database V1 conhecido migra atomicamente; rollback exige o backup verificado
anterior à migração.

Um store cria handles independentes para replication log, outbox, checkpoints e
tombstones opacos. Clones compartilham um pool de no máximo 32 conexões. A
admissão de writers e o busy wait têm deadline. Reads, snapshots, entries de
outbox, tombstones, páginas e etapas de backup possuem limites explícitos.

A admissão na outbox calcula o tamanho exato do JSON canônico sem cópia
codificada e transmite o record para um BLOB incremental do SQLite. Verificação
de duplicatas, reads de página e validação no startup também transmitem o BLOB;
assim a mensagem owned nunca compartilha memória com um segundo buffer
codificado do tamanho do record. O scratch de leitura e escrita acompanha o
tamanho codificado e é limitado a 64 KiB e 1 MiB, respectivamente; records
pequenos não reservam buffers máximos.

Snapshots portáveis usam `ReplicationSnapshot` V1. Backup online usa a API de
backup do SQLite e publica apenas arquivo novo verificado. Restore também exige
um destino novo; substituir database em uso não é suportado. Mantenha database,
`-wal` e `-shm` juntos até o fechamento de todos os handles.

A criação do snapshot transfere as alocações dos payloads do database para o
valor portável. O restore portável valida esse valor por referência, rejeita
bytes de payload agregados acima de `max_database_bytes` antes de remover
qualquer row e empresta os records durante sua única transação de substituição.
No Apple M1, o workload de restore de 32 MiB mediu 396,00 ms p50 e 73,84 MiB de
RSS pico, contra 466,80 ms e 108,97 MiB com duas réplicas temporárias dos
payloads.

SQLite suporta processos locais independentes em filesystem com locking
confiável. Shares de rede e hosts concorrentes estão fora deste perfil. O
provider não contém schema de aplicação nem oferece escape de SQL arbitrário.

Para rollback, pare admissão, drene/exporte a outbox, crie backup verificado e
exporte um snapshot portável. A persistência em arquivos deve ser criada
explicitamente; renomear o database não é migration.

Cada conexão do pool e conexão auxiliar de backup/restore configura e confere
`cache_size=-2048` e `mmap_size=0`. O cache é uma meta sugerida de 2 MiB, não
um teto rígido de heap. Oito conexões padrão representam cerca de 16 MiB de
metas de cache, antes de conexões auxiliares, consultas, temporários, WAL,
payloads e overhead do allocator. Nenhuma política global de heap SQLite é
alterada. Temporários e crescimento do WAL ainda exigem orçamento do deployment.
Veja [a semântica do cache SQLite](https://www.sqlite.org/pragma.html#pragma_cache_size).

As conexões também solicitam e conferem `temp_store=FILE` antes do uso, sem
alterar o diretório temporário global. Isso não garante que todo trabalho
temporário vá ao disco: `SQLITE_TEMP_STORE=3` sobrepõe a opção (o build bundled
Android usa isso). SQLite também pode reter páginas temporárias em cache.
O deployment deve conferir seu build e prever espaço temporário privado,
memória e limpeza; builds somente em memória exigem orçamento próprio.
Veja [temporários no SQLite](https://www.sqlite.org/pragma.html#pragma_temp_store).

Autocheckpoint do WAL em 1.000 páginas é um gatilho, não teto de disco. Um
leitor com transação aberta pode impedir checkpoint completo enquanto writers
ampliam o WAL. O teste interno mantém um leitor durante oito appends de 1 MiB,
observa mais de 1.000 frames e verifica truncamento e registros íntegros após
liberar o leitor e reabrir. Ele usa SQL privado, sem adicionar API pública de
checkpoint. Limite leitores/backups e monitore WAL e capacidade do filesystem
no deployment; nunca apague um WAL ativo para recuperar espaço.

Páginas do replication log agora validam a soma de `length(payload)` antes de
converter qualquer BLOB da página em `Vec` Rust. Quantidade, metadata e payloads
são lidos na mesma transação deferred, impedindo que substituição concorrente
mude a página entre as etapas. A página selecionada acima do limite continua
falhando inteira, como antes. Há uma passagem adicional de metadata; isso não
limita o cache interno do SQLite nem fornece saída streaming.

## Certificação

O benchmark release com fonte limpa em `0f6f6d0` passou em macOS arm64 com Rust
1.97.1. Em 2.048 appends duráveis de 1 KiB e 2.048 leituras pontuais, o p99 de
append foi 1,086 ms a 3.729 operações/s e o p99 de leitura foi 0,583 ms a 6.578
operações/s. O backup online verificado de 3.182.592 bytes levou 73,870 ms; a
verificação integral levou 15,675 ms. A reprodução usa
`appcore-certification bottlenecks`, conforme
`release/sqlite-sync-provider-v1.md`. No workload atual de 512 entries pequenas,
o enqueue solicitou 255.676 bytes do heap Rust, sem retenção, e mediu 141.791 ns
p99, abaixo dos gates explícitos de 2 MiB e 250 ms. O scratch proporcional
reduziu os bytes solicitados no workload SQLite completo de 578.081.344 para
8.251.670 (-98,57%) e o delta de heap vivo de 1.083.528 para 233.600 bytes
(-78,44%).

O runner do crate também exercita caminhos de dados grandes. Em Apple M1,
macOS 27, três processos medidos e um warmup, o enqueue raw de 16 MiB teve p50
249,76 ms, pico RSS 42,81 MiB e delta RSS de workload 19,62 MiB. O restore de
snapshot com 32 registros/32 MiB teve p50 361,29 ms, pico RSS 73,95 MiB e delta
de workload 2,55 MiB; o snapshot preparado já existia no checkpoint idle. O
relatório não versionado é `target/appcore/bench/sync-sqlite-memory.json`.
