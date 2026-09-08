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
ordenado exato em uma transação. O enqueue mede primeiro o tamanho JSON canônico
exato e escreve diretamente em um `zeroblob`; comparação de duplicatas, reads
de páginas e validação de integridade no startup também transmitem o conteúdo
dos BLOBs. Nenhum `Vec<u8>` do tamanho do record codificado coexiste com a
mensagem owned. Os buffers do stream usam o tamanho do record codificado até os
tetos fixos de 64 KiB para leitura e 1 MiB para escrita; um record pequeno nunca
reserva esses máximos. Um database schema V1 conhecido migra
atomicamente com metadata de retry zerada. Preserve backup anterior para
rollback.

A criação do snapshot portável move para o snapshot V1 os payloads lidos do
SQLite. O restore valida por referência o snapshot owned pelo caller, compara o
payload agregado com `max_database_bytes` antes da mutação e insere diretamente
esses records emprestados em uma transação. Nenhum replication log completo em
memória nem segunda coleção de payloads coexiste com o snapshot.

O descriptor declara `transactions`, `locking`, `snapshot`, `online_backup` e
`multi_process`. Ele não declara `streaming` nem `multi_host`.

Este crate em desenvolvimento não é selecionado por manifests V1 estaveis e
não está ligado ao SDK. Consumers diretos fazem opt-in explícito. Veja
[`release/sqlite-sync-provider-v1.md`](../../release/sqlite-sync-provider-v1.md).

```bash
cargo test -p appcore-sync-sqlite
```

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

## Documentação estável

ID estável: **ACR-026**. Consulte o
[guia complementar de arquitetura e integração](https://wiki.appcore.dnettoraw.com/pt/crates/id/acr-026). Esse ID permanente
continua válido se a página da wiki mudar.
