# appcore-sync-sqlite

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Persistência SQLite opcional pós-1.0 para estado de sincronização AppCore.

O crate implementa os contratos existentes de replication log, outbox e
checkpoint. Também fornece snapshots portáveis, tombstones opacos limitados,
inspeção de integridade e backup/restore online verificado. Ele nunca expõe a
conexão SQLite nem aceita SQL, tabelas, migrations ou workflows da aplicação.

Cada database usa schema interno V2 transacional, WAL, `synchronous=FULL`, pool
de conexões limitado, busy timeout, limites do SQLite e validação de integridade
no startup. Schemas desconhecidos, sem versão ou futuros falham com
`NO MORE SUPPORTED PLEASE UPDATE`.

O schema V2 adiciona attempts limitados e timestamps de readiness à outbox.
`peek` e `next_ready` selecionam metadata de quantidade/bytes antes de ler
BLOBs; stats não carregam payload e receipt parcial remove somente um prefixo
ordenado exato em uma transação. Um database schema V1 conhecido migra
atomicamente com metadata de retry zerada. Preserve backup anterior para
rollback.

O descriptor declara `transactions`, `locking`, `snapshot`, `online_backup` e
`multi_process`. Ele não declara `streaming` nem `multi_host`.

Este crate em desenvolvimento não é selecionado por manifests V1 congelados e
não está ligado ao `appcore-bin`. Consumers diretos fazem opt-in explícito. Veja
[`release/sqlite-sync-provider-v1.md`](../../release/sqlite-sync-provider-v1.md).

```bash
cargo test -p appcore-sync-sqlite
```
