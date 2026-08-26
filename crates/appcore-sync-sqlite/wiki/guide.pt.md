# appcore-sync-sqlite

[English](guide.en.md) | [Français](guide.fr.md) |
[Básico](examples/basic.pt.md) | [Intermediário](examples/intermediate.pt.md)

**Camada:** integration. **Status:** `0.1.0-alpha.2` prerelease publicada.

`SqliteSyncStore::open` resolve o path para um local estável, rejeita target de
database por symlink, configura WAL e limites do SQLite, executa somente
migrations transacionais conhecidas e verifica integridade antes de retornar.
Corrupção completa e formatos desconhecidos falham fechados com erros redigidos.

Um store cria handles independentes para replication log, outbox, checkpoints e
tombstones opacos. Clones compartilham um pool de no máximo 32 conexões. A
admissão de writers e o busy wait têm deadline. Reads, snapshots, entries de
outbox, tombstones, páginas e etapas de backup possuem limites explícitos.

Snapshots portáveis usam `ReplicationSnapshot` V1. Backup online usa a API de
backup do SQLite e publica apenas arquivo novo verificado. Restore também exige
um destino novo; substituir database em uso não é suportado. Mantenha database,
`-wal` e `-shm` juntos até o fechamento de todos os handles.

SQLite suporta processos locais independentes em filesystem com locking
confiável. Shares de rede e hosts concorrentes estão fora deste perfil. O
provider não contém schema de aplicação nem oferece escape de SQL arbitrário.

Para rollback, pare admissão, drene/exporte a outbox, crie backup verificado e
exporte um snapshot portável. A persistência em arquivos deve ser criada
explicitamente; renomear o database não é migration.

## Certificação

O benchmark release com fonte limpa em `0f6f6d0` passou em macOS arm64 com Rust
1.97.1. Em 2.048 appends duráveis de 1 KiB e 2.048 leituras pontuais, o p99 de
append foi 1,086 ms a 3.729 operações/s e o p99 de leitura foi 0,583 ms a 6.578
operações/s. O backup online verificado de 3.182.592 bytes levou 73,870 ms; a
verificação integral levou 15,675 ms. A reprodução usa
`appcore-certification bottlenecks`, conforme
`release/sqlite-sync-provider-v1.md`.
