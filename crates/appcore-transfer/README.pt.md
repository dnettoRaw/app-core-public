# appcore-transfer

A beta `0.1.0-beta.1` transfere objetos opacos imutáveis de fontes explicitamente aprovadas. Os descritores devem vir de uma aplicação ou deployment confiável. Integridade SHA-256 não autentica uma release.

[Guide](wiki/guide.pt.md) · [Basic](wiki/examples/basic.pt.md) · [Intermediate](wiki/examples/intermediate.pt.md)

Default: object 64 MiB, block 64 KiB, store 256 MiB, 16,384 files. Hard maxima: object 256 MiB, block 1 MiB, 4,096 blocks, 65,536 files, 16 sources, 8 workers, 32 attempts/block. Assembly needs space for both blocks and complete content. Descriptor JSON is bounded to 1 MiB. Each worker retains at most one block; descriptor and inventory are bounded. Local filesystem operations require deployment-owned I/O bounds.

Ative peer-rpc para usar PeerBlockSource e TransferDispatcher pelos streams Peer RPC V2 assinados existentes. O repositório deve autorizar tenant, cluster, origem, destino e objeto. Configure timeouts de I/O dentro do prazo cooperativo da transferência. Providers devem respeitar cancelamento; todos os workers são aguardados. Descoberta e relays pertencem ao deployment; Sync controla replicação e Update autenticidade, ativação e rollback.

## Criação com token elevado e guards Windows

Esta secção descreve alterações ainda não publicadas do código-fonte. Verifique o pacote exato do registry antes da adoção pelos consumidores.

Apenas ficheiros/diretórios privados novos recebem owner explícito do utilizador atual e DACL protegida antes da validação estrita. Paths inseguros existentes continuam recusados. Roots e sessões de objetos Windows conservam guards Security contra substituição de ancestrais/diretório do objeto durante operações. Security é dependência apenas Windows; `peer-rpc` permanece opcional e não há dependência Update. Testes nativos exigem tokens limitado/elevado; fixtures usam paths Temp Windows sem prefixo verbatim. O path de objeto verificado após drop do store continua sob proteção do deployment e é revalidado ao abrir.

## Candidato de release — 2026-10-14

`0.1.0-beta.2`: alterações próprias.
