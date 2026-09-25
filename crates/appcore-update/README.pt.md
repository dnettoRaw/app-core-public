# appcore-update

Testes locais:

```bash
cargo test -p appcore-update
```

**Responsabilidade:** seleção, autenticidade, stage, ativação, health gate e
rollback de artefato opaco.

O contrato adicional `ReleaseCatalog` valida descritores assinados e limitados
por plataforma e seleciona a maior versão compatível para uma
`UpdateIdentity`. Descritores V1 sem target continuam válidos; entradas de
catálogo exigem `ArtifactTarget`, cujos campos entram no payload da assinatura.
As trust roots sempre são fornecidas pela política do deployment.

`LatestCompatibleOfferRequestV2` e `LatestCompatibleOfferResponseV2`
selecionam a maior release mais nova que seja compatível com target, versão do
Runtime e protocolo. O peer devolve o descriptor assinado; o cliente deve
chamar `verify_descriptor` com sua política local de confiança antes de fazer
stage ou buscar bytes.

`ReleaseCatalogStore` abre um catálogo JSON limitado dentro de uma raiz
controlada. Cada entrada mantém a localização relativa segura separada do
descriptor assinado. O store rejeita traversal, symlinks, arquivos não
regulares, hashes duplicados e descriptors ausentes do catálogo. Ele implementa
`ArtifactSource` e lê somente o tamanho declarado do artefato.

`ActivationAdapter` define a fronteira do host para `prepare`, `activate`,
`healthcheck`, `commit`, `rollback` e `recover`. `ActivationRequest` e
`ActivationEvidence` são limitados e podem virar um `ActivationReceiptV2`;
instaladores desktop, raw-binary e Docker continuam fora do Runtime. Falhas do
adapter devem virar ações explícitas de recovery e nunca provocar rollback
inferido.

`ArtifactSource`, `ArtifactWriter` e `receive_artifact` fornecem um caminho de
transferência síncrono e limitado. O receiver verifica offsets, tamanho dos
blocos, tamanho declarado e SHA-256 final, sem ativar ou instalar o resultado.

`UpdateCache` adiciona stage retomável por hash, lock exclusivo entre
processos, reserva de quota, recuperação de arquivos parciais e publicação
durável do descriptor e objeto. Ele permanece separado do `FileArtifactStore`;
com `secure_permissions`, o cache rejeita diretórios e ancestrais com symlink
ou escrita insegura, sem repará-los. A fronteira reutilizável de handles/ACLs é
exposta por `appcore-security`; a parede de camadas impede dependência direta.

Os metadados de transferência Peer são expostos em contratos `offer` e
`chunk` sem paths, para streams autenticados do Peer RPC V2. O transporte
continua responsável por autenticação, isolamento tenant/cluster, deadlines,
sequência e hashes dos bytes decodificados; `appcore-update` valida identidade
do artefato, offsets e os metadados repetidos de tamanho/digest.

A API adicional de recovery V2 usa `ActivationReceiptV2`,
`FileRecoveryStore`, `inspect_recovery` e ações explícitas de `replay`. Ela
mantém o receipt V1 inalterado, protege ações por tentativa e digest e nunca
faz rollback externo implicitamente.

`QuarantineStore` registra releases que falharam usando aplicação, canal,
versão, build e SHA-256 como uma chave bounded. Entradas têm motivos tipados e
timestamps, sobrevivem a reinício e downgrade e permanecem ativas até uma
chamada explícita a `release` ou até a expiração configurada. Use
`select_with_quarantine_report` para obter o descriptor selecionado e as
exclusões limitadas com chave da release, motivo e expiração.

O binário `appcore-update-diagnose` é somente leitura. Ele inspeciona
descriptor, diretório de receipts, diretório de quarentena ou cache com saída
humana ou `--json`. O JSON usa `schema_version: 1`; referências de artefato,
assinaturas e host bindings são redigidos.

A árvore `fixtures/` e os testes de integração fornecem evidência portátil para
cache parcial/corrompido, catálogos ambíguos, receipts incompletos e escritas
concorrentes bounded na quarentena.

**Dependências internas:** `appcore-contracts` e `appcore-provider`.

**API principal:** artifact descriptor/signing payload; verifier,
unsigned-local protegido por feature/Ed25519, trust policy/key status; update request/provider e file
factory; staged artifact, activation receipt/store; coordinator,
preparation/outcome, health check e fault injection. Ed25519 verifica artefatos
assinados. Artefatos locais sem assinatura exigem a feature explícita
`allow-unsigned-local-artifacts` e uma raiz local controlada pelo proprietário;
não são fallback para supply chain remoto.

Use para binários ou artefatos opacos. O Runtime valida identidade, versão,
protocolo, checksum e trust, sem entender código ou schema.
Migrações de schema pertencem à aplicação.

Leituras de arquivo verificam tamanho antes de alocar, usam scratch fixo de 16
KiB mais um byte sentinela não retido e rejeitam componente final não regular.
A ativação transmite o staged por um buffer SHA-256 fixo de 64 KiB, sem
materializar o arquivo, e cria hard link para um path de build imutável. Um
path existente só é reutilizado quando tamanho e digest correspondem
exatamente ao descriptor; nunca é substituído. O no-follow atômico do
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

## Documentação estável

ID estável: **ACR-021**. Consulte o
[guia complementar de arquitetura e integração](https://wiki.appcore.dnettoraw.com/pt/crates/id/acr-021). Esse ID permanente
continua válido se a página da wiki mudar.

`MobileUpdatePolicy` avalia um pedido mobile limitado antes de anunciar uma
oferta. Ela distingue substituição própria, loja, MDM, sideload assistido pelo
deployment e não suportado, bloqueia protocolos obsoletos e exige uma versão
mínima do cluster. Nunca instala artefatos nem recomenda contornar a política
da App Store/Play Store; essas ações continuam sob responsabilidade do
deployment.
