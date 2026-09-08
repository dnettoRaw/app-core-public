# appcore-update

[Exemplo minimo](examples/basic.pt.md) |
[Exemplo intermediario](examples/intermediate.pt.md)

**Responsabilidade:** seleção, autenticidade, stage, ativação, health gate e
rollback de artefato opaco.

**Dependências internas:** contracts e provider.

**API principal:** artifact descriptor/signing payload; verifier,
unsigned-local protegido por feature/Ed25519, trust policy/key status; update request/provider e file
factory; staged artifact, activation receipt/store; coordinator,
preparation/outcome, health check e fault injection.

Use para binários ou artefatos opacos. O Runtime valida identidade, versão,
protocolo, checksum e trust, sem entender código ou schema.

Leituras de arquivo verificam tamanho antes de alocar, usam scratch fixo de 16
KiB mais um byte sentinela não retido e rejeitam componente final não regular.
A ativação transmite a validação de tamanho e SHA-256 por um buffer fixo de 64
KiB e cria hard link para um path de build imutável. Um path existente só é
reutilizado quando tamanho e digest correspondem exatamente ao descriptor;
nunca é substituído. O no-follow atômico do
componente final existe em Unix. Outras plataformas mantêm checks de metadata,
mas dependem da fronteira do filesystem do deployment contra races de reparse.

Ponteiros active/previous e receipts de ativação pendentes emprestam seus
descriptors, passam por sizing sem retenção sob 1 MiB e serializam diretamente
no temporário atômico com buffer fixo de 16 KiB. As leituras também
desserializam diretamente por um reader limitado fixo de 16 KiB, sem manter um
vetor completo dos bytes codificados junto ao pointer ou receipt decodificado.
Arquivo ausente, falha de I/O e falha de decode continuam distintos, preservando
o upgrade wall da ativação pendente. O JSON V1 não mudou.

O file provider percorre o índice limitado em streaming uma vez e retém somente
a melhor versão semântica e seu descriptor. Cada descriptor é validado e então
descartado ou selecionado durante a decodificação do array JSON, sem vetor de
descriptors nem lista ordenada de candidatos. Versões iguais preservam a
primeira entrada. Um reader fixo de 16 KiB, preflight de 1 MiB e um byte
sentinela não retido rejeitam tamanho declarado excessivo e crescimento
concorrente.

**Maturidade:** lifecycle RC estável; supply chain remoto exige assinatura,
provenance e trust roots.
