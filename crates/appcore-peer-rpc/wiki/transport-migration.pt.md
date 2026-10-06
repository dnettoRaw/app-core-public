# Envelope de aplicação V1 para Peer RPC streaming V2

Versões de envelope/schema da aplicação, wire do sync e transporte Peer RPC são
independentes. Streaming V2 não atualiza schema de domínio nem muda semântica
líder-seguidor. Chunks/streams já existem; migração exige adaptadores externos
explícitos da aplicação.

## Mapeamento e contrato de deployment

| Fronteira da aplicação | Fronteira streaming | Responsabilidade preservada |
| --- | --- | --- |
| Negociação / status V1 | Query de capability versionada da aplicação | Declarar transporte suportado; ausência significa não elegível para V2 |
| `IngestChunkV1` | Stream open/chunk/commit verificado com payload da aplicação | Adaptador externo reconstrói/valida bytes, identidade, sequência e digest |
| `PullRequestV1` / `PullChunkV1` | Command/query explícito e resposta pull limitada | Preservar cursor, checkpoint e autorização fora dos frames |
| Páginas de snapshot / eventos | Payloads opacos limitados | Aplicação valida schema e publica somente resultado completo verificado |

O pacote externo de protocolo deve conter o mapeamento e fixtures golden. Manter
endpoints V1 da aplicação para clientes antigos somente nesse pacote externo.
Runtime V1 não infere V2, não lê envelopes removidos nem converte formatos da
aplicação. Entradas removidas do Runtime continuam retornando
`NO MORE SUPPORTED PLEASE UPDATE`.

## Migração em etapas

1. Inventariar versões instaladas, identidades, scopes, limites, retries e
   outboxes/checkpoints persistidos. Registrar fixtures V1 no repositório da
   aplicação; schemas de negócio não entram no Runtime.
2. Adicionar capability versionada explícita ao adaptador externo. Configurar
   `PeerRpcStreamRegistry` em spool existente owner-only e instalar com
   `PeerRpcHttpHost::with_v2_stream_registry`. Definir limites por capability e
   validar tenant, cluster, origem e destino antes do dispatch.
3. Clientes V1 da aplicação mantêm caminho explícito atual. Selecionar V2 apenas
   após negociação validada ou setting explícito de deployment. Host Runtime
   padrão permanece V1. JSON canônico V2 é distinto do binário: este exige host
   `with_v2_binary_codec` e client
   `with_stream_codec_v2(PeerRpcStreamCodecV2::Binary)` juntos.
4. Provar payload/resultado e checkpoints equivalentes por transporte. Testar
   timeout, cancelamento, replay, duplicação, corrupção, oversize, isolamento,
   perda de peer e commit ambíguo. Não repetir frames automaticamente após falha
   ambígua; reconciliar receipts da aplicação com a mesma chave de idempotência.
5. Atualizar receivers primeiro, depois senders selecionados. Monitorar códigos
   seguros, saturação, limpeza do spool e atraso de checkpoint. Cohorts explícitos;
   não fazer downgrade silencioso após erro de autenticação ou media type.
6. Rollback: parar novos sends V2, drenar/expirar sessões, reconciliar receipts
   e preservar outboxes/checkpoints. Retomar adaptador V1 externo explicitamente
   somente com schema/estado compatíveis. Persistência incompatível exige plano
   revisado de versão/migração/rollback; não apagar nem converter por inferência.

## Evidência de conclusão

Evidência da aplicação: cliente antigo/receiver novo, sender novo/receiver novo,
par antigo selecionado explicitamente e recusa controlada dos pares não
suportados. Exigir payloads golden equivalentes, replay sem publicação duplicada,
falhas sem reserva pending permanente, snapshots sem publicação parcial,
recursos limitados e recovery após restart. Testes do Runtime certificam
primitivas, não o mapeamento externo. Este documento não fornece o adaptador.
