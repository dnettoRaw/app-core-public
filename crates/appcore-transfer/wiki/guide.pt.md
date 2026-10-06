# Transferência limitada e verificada

A beta `0.1.0-beta.1` transfere objetos opacos imutáveis de fontes explicitamente aprovadas. Os descritores devem vir de uma aplicação ou deployment confiável. Integridade SHA-256 não autentica uma release.

## Recuperação e limites

É necessária uma raiz privada existente: Unix 0700 do proprietário, ou ACL Windows exclusiva do proprietário. O armazenamento mantém lock exclusivo entre processos. Blocos verificados sobrevivem ao reinício; corrupção e versões desconhecidas falham de forma fechada. Temporários órfãos reconhecidos são removidos sob lock. TTL remove apenas objetos incompletos. A retenção dos objetos completos pertence ao deployment.

Default: object 64 MiB, block 64 KiB, store 256 MiB, 16,384 files. Hard maxima: object 256 MiB, block 1 MiB, 4,096 blocks, 65,536 files, 16 sources, 8 workers, 32 attempts/block. Assembly needs space for both blocks and complete content. Descriptor JSON is bounded to 1 MiB. Each worker retains at most one block; descriptor and inventory are bounded. Local filesystem operations require deployment-owned I/O bounds.

## Transporte e responsabilidades

Ative peer-rpc para usar PeerBlockSource e TransferDispatcher pelos streams Peer RPC V2 assinados existentes. O repositório deve autorizar tenant, cluster, origem, destino e objeto. Configure timeouts de I/O dentro do prazo cooperativo da transferência. Providers devem respeitar cancelamento; todos os workers são aguardados. Descoberta e relays pertencem ao deployment; Sync controla replicação e Update autenticidade, ativação e rollback.

Windows cross-compilation is checked; native Windows recovery and physical mesh performance remain release qualification work. MCU remains planned. No installation or artifact activation occurs here.
