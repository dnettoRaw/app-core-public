# appcore-transfer

Release candidate **`appcore-transfer 1.0.0-rc.1`**. Selecionar esta RC explicitamente; não substitui versões estáveis anteriores nem certifica um deployment. Quando existem, as dependências AppCore internas estão fixadas nesta rodada RC.


A beta `0.1.0-beta.1` transfere objetos opacos imutáveis de fontes explicitamente aprovadas. Os descritores devem vir de uma aplicação ou deployment confiável. Integridade SHA-256 não autentica uma release.

[Guide](wiki/guide.pt.md) · [Basic](wiki/examples/basic.pt.md) · [Intermediate](wiki/examples/intermediate.pt.md)

Default: object 64 MiB, block 64 KiB, store 256 MiB, 16,384 files. Hard maxima: object 256 MiB, block 1 MiB, 4,096 blocks, 65,536 files, 16 sources, 8 workers, 32 attempts/block. Assembly needs space for both blocks and complete content. Descriptor JSON is bounded to 1 MiB. Each worker retains at most one block; descriptor and inventory are bounded. Local filesystem operations require deployment-owned I/O bounds.

Ative peer-rpc para usar PeerBlockSource e TransferDispatcher pelos streams Peer RPC V2 assinados existentes. O repositório deve autorizar tenant, cluster, origem, destino e objeto. Configure timeouts de I/O dentro do prazo cooperativo da transferência. Providers devem respeitar cancelamento; todos os workers são aguardados. Descoberta e relays pertencem ao deployment; Sync controla replicação e Update autenticidade, ativação e rollback.

## Criação com token elevado e guards Windows

Esta secção descreve alterações ainda não publicadas do código-fonte. Verifique o pacote exato do registry antes da adoção pelos consumidores.

Apenas ficheiros/diretórios privados novos recebem owner explícito do utilizador atual e DACL protegida antes da validação estrita. Paths inseguros existentes continuam recusados. Roots e sessões de objetos Windows conservam guards Security contra substituição de ancestrais/diretório do objeto durante operações. Security é dependência apenas Windows; `peer-rpc` permanece opcional e não há dependência Update. Testes nativos exigem tokens limitado/elevado; fixtures usam paths Temp Windows sem prefixo verbatim. O path de objeto verificado após drop do store continua sob proteção do deployment e é revalidado ao abrir.

## Candidato de release — 2026-10-14

`0.1.0-beta.2`: alterações próprias.

## Política atual de fontes (fonte não publicada)

`AuthorizedBlockSource`, `TransferSourceScopeV1` e `TransferSourceAuthorizer` acrescentam uma fronteira explícita de política ao provider escolhido. Scope vincula tenant, cluster, Core requerente e Core fornecedor (perfil ASCII de 128 bytes); callback recebe descriptor completo e `TransferControl`. A implementação verifica autorização atual do objeto/redistribuição, expiração/revogação e disponibilidade da autoridade dentro do deadline restante. Autoridade ausente recusa acesso. Discovery continua no deployment: resolver anúncios/identidades Core autenticados e limitados para endpoints, autorizar descriptor exato e escolher até 16 providers distintos com scope. Disponibilidade anunciada ou integridade SHA-256 não autoriza redistribuição.

O wrapper verifica política antes de pedir cada bloco e antes de o engine aceitar bytes em buffer. Recusa põe a fonte em quarentena nesta invocação e roda para outra selecionada, conservando blocos anteriores verificados. Se todas recusarem, `transfer` devolve `TransferError::Unauthorized`; uma recusa nunca conta como segunda fonte aprovada. Limites existentes de retry/concorrência/storage mantêm-se. `quarantined_sources` passa a incluir recusas de política. Peer RPC V2 mapeia recusas tipadas de autorização/identidade para Unauthorized, hashes inválidos do stream para Integrity e transporte para Source, sem logs de detalhes remotos nem mudar protocolo.

Usar wrappers através de `transfer`: o buffer descarta bytes se a verificação final falhar. Sink direto arbitrário pode já ter recebido bytes antes da segunda verificação. Checks são observações, não revogação remota atómica; pedido em curso já autorizado não pode ser impedido retroativamente. `TransferRepository::open_authorized` no servidor autenticado continua obrigatório e independente da política cliente. Sem fallback implícito de endpoint/protocolo nem instalação.

APIs aditivas não publicadas. `TransferError` é não exaustivo; falhas de autorização antes reduzidas a esgotamento genérico podem agora devolver Unauthorized. Atualizar diagnósticos do consumidor explicitamente. Formatos persistidos, wire V2 e `SourceBinding::approved` existentes mantêm-se; o construtor de nível inferior continua a ser asserção confiável do chamador. Execução ACL Windows limited/elevated e evidência DNET real são qualificação separada, não comprovada por testes locais.

Checks de fonte governam novas leituras do provider. Concluir apenas com blocos locais já verificados não contacta nem reautoriza providers; o deployment autoriza separadamente uso do cache e cada pedido de redistribuição no servidor.
