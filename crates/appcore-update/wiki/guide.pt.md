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

`ReleaseCatalog` é um contrato adicional para targets de plataforma assinados.
Use `ArtifactTarget` para sistema operacional, arquitetura e formato do host;
o target é coberto pela assinatura do descriptor. O catálogo valida todas as
entradas antes da seleção, rejeita chaves duplicadas de identidade/canal/
target/versão e nunca fornece suas próprias trust roots. Descritores V1
continuam utilizáveis fora do catálogo.

Para seleção iniciada por peer, use `LatestCompatibleOfferRequestV2` e
`LatestCompatibleOfferResponseV2`. A seleção exclui versões iguais ou antigas
e verifica target, canal, versão do Runtime e protocolo. O peer não substitui a
política local de confiança: chame `verify_descriptor` com o
`ArtifactAuthenticityVerifier` do deployment antes de aceitar o descriptor.

Use `ReleaseCatalogStore` quando os bytes locais forem publicados junto do
catálogo. As entradas mantêm a localização relativa segura fora do descriptor
assinado, e os chunks só são servidos quando o descriptor completo está no
catálogo validado. A raiz controlada rejeita traversal, symlinks, arquivos não
regulares e hashes duplicados ambíguos.

A ativação do host é expressa por `ActivationAdapter`: implemente `prepare`,
`activate`, `healthcheck`, `commit`, `rollback` e `recover` para o deployment.
`ActivationRequest` e `ActivationEvidence` convertem para a fronteira do
receipt V2; instaladores de plataforma ficam fora do AppCore e as ações de
recovery são explícitas.

Para transferência limitada, implemente `ArtifactSource` e `ArtifactWriter` e
use `receive_artifact`. O receiver valida o offset solicitado, o limite do
bloco, o tamanho declarado e o SHA-256 final. Receber permanece separado de
stage, ativação e instalação.

Use `UpdateCache` para stage retomável de downloads. Ele possui arquivos
`.part`, objetos endereçados por hash, metadata do descriptor, lock entre
processos e quota. Não ativa artefatos nem remove releases protegidas;
`FileArtifactStore` continua sendo o store de instalação e rollback. Com
`secure_permissions`, ele rejeita diretórios e ancestrais com symlink ou escrita
insegura sem repará-los. A fronteira reutilizável de handles/ACLs fica em
`appcore-security`, enquanto a parede de camadas mantém este crate independente.

Para Peer RPC V2, transporte `ArtifactOfferRequestV2` e
`ArtifactChunkRequestV2` como payloads de request limitados e repita os
metadados de resposta em `ArtifactOfferResponseV2` ou
`ArtifactChunkResponseV2`. Esses payloads não contêm path remoto. Use o stream
V2 existente para os bytes; autorização e framing do transporte permanecem
fora deste crate.

Para recovery, crie um `ActivationReceiptV2` em `FileRecoveryStore`, chame
`inspect_recovery` no startup e aplique somente uma `RecoveryAction` explícita.
`replay` protege a ação por `attempt_id` e digest. O host deve executar e
observar qualquer rollback externo antes de registrá-lo; a semântica V1
continua inalterada.

Use `QuarantineStore` após uma falha de healthcheck ou ativação. Sua chave
bounded inclui aplicação, canal, versão, build e digest. A quarentena é
durável, sobrevive a downgrade e reinício e só pode ser liberada pela operação
explícita `release`. `quarantine_until` suporta expiração exclusiva;
`select_with_quarantine_report` explica exclusões ativas com motivo, build e
chave SHA-256. Entradas expiradas permanecem no diagnóstico, mas não bloqueiam
a seleção.

Execute `appcore-update-diagnose --json descriptor <arquivo>`,
`receipt <diretório>`, `quarantine <diretório>` ou `cache <diretório>` para
inspeção segura em CI. A ferramenta nunca ativa, corrige, libera ou remove
nada. As classes de saída são estáveis: `64` uso, `65` dados inválidos, `66`
entrada ausente e `74` I/O.

As fixtures cobrem cache parcial e corrompido, seleção de catálogo ambígua e
receipts de recovery incompletos. O harness também exercita o lock de processo
com escritas concorrentes bounded na quarentena; durabilidade nativa de
filesystem continua sendo evidência específica de cada plataforma.

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

Para mobile, construa `MobileUpdatePolicy` com a ação do deployment, a versão
mínima do cluster e o protocolo exigido. Avalie versão instalada, candidato,
cluster e target antes de anunciar disponibilidade. Protocolo incompatível
bloqueia o cliente; cluster antigo bloqueia a oferta. O contrato apenas relata
a política e nunca executa ações de loja, MDM ou sideload.
