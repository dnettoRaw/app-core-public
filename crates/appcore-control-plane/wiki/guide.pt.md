# appcore-control-plane

Release candidate **`appcore-control-plane 1.0.4-rc.4`**. Selecionar esta RC explicitamente; não substitui versões estáveis anteriores nem certifica um deployment. Quando existem, as dependências AppCore internas estão fixadas nesta rodada RC.


Descartar a future antes do despacho impede a execução da operação. A fila
mantém referência fraca ao resultado, sem reter waker ou resposta de uma future
abandonada. A closure e o slot permanecem até sair da fila. Após o despacho,
descartar a future não desfaz efeitos remotos nem interrompe transporte síncrono;
use cancelamento cooperativo e reconcilie resultados ambíguos.

Aquisição/renovação e liberação de lease usam uma tentativa HTTP, independentemente
da configuração de retries. V1 não fornece chave de deduplicação remota; a
resposta pode se perder após aplicar a mutação. Timeout ou falha HTTP transitória
não provam que o lease ficou inalterado. Reconcilie estado autoritativo e fencing
antes de escritas dependentes de liderança; não repita a operação cegamente.
Discovery, registro e heartbeat mantêm retries configurados; a semântica remota
ainda exige testes de conformidade do deployment.

As esperas de retry usam jitter não criptográfico entre metade (arredondada
para cima) e o teto exponencial atual, limitadas também ao tempo restante.
Backoff zero continua zero. Cada ciclo inicializa o estado local com o hasher
aleatorizado da biblioteca padrão; isso não é aleatoriedade de segurança.

A configuração de retries HTTP é limitada ao executar o request: até 16
tentativas, timeout de 1–30.000 ms e backoff de até 30.000 ms. Zero tentativas
continua significando uma; backoff zero é permitido. O orçamento conservador é
tentativas × timeout + (tentativas − 1) × backoff máximo, limitado a 120 segundos.
Relógio monotônico limita cada tentativa e espera ao tempo restante. Uma resposta
tardia retorna Timeout sem nova tentativa, mesmo se indicar sucesso; a operação
remota pode já ter sido aplicada. Transportes externos devem respeitar o prazo:
callbacks síncronos não podem ser interrompidos à força. O orçamento começa
na chamada do provider e inclui fila, encode e decode. Requests vencidos na
fila não chegam ao transporte. O vencimento é observado quando o worker avança,
sem timer independente: um transporte anterior preso pode atrasar a future.

Retries HTTP se limitam às respostas 408, 429, 500, 502, 503 e 504 e a
falhas de transporte, timeout ou offline. Outros status e falhas semânticas
tipadas retornam imediatamente. O primeiro backoff também respeita
`max_backoff_ms`. Essa política não estabelece prazo total da operação nem
prova que uma escrita de resultado ambíguo pode ser repetida com segurança.

[Exemplo minimo](examples/basic.pt.md) |
[Exemplo intermediario](examples/intermediate.pt.md)

**Responsabilidade:** implementações genéricas de presença, heartbeat, discovery
e leases.

**Dependências internas:** contracts, core, distributed contracts e transport.

**API principal:** clients in-memory, file e offline; configuração HTTP, retry
policy e transport trait; transports one-shot standard, pooled e bearer;
coordinator e heartbeat policy; guards de liderança global/serviço; validação
de endpoint seguro.

Use `PooledHttpTransport` para chamadas reutilizáveis sem autenticação.
`BearerHttpTransport` também possui um cliente reutilizável e limitado.
Mantenha `StdHttpTransport` somente onde o comportamento V1 one-shot com
`Connection: close` for necessário.
`HttpControlPlaneClient` converte um body codificado uma vez em
`SharedHttpControlPlaneRequest` e o empresta em todos os retries limitados. Os
transports internos clonam somente seu owner compartilhado; transports externos
existentes que implementam o método owned continuam pelo default compatível.
Os dois owners de request omitem os bytes do body no `Debug`.

Use para coordenação distribuída sem payload de negócio. Perfil file exige
locks/storage certificados. Perfil remoto exige TLS e autenticação do
deployment.

O perfil file limita estado e backup a 16 MiB e rejeita estado malformado ou
futuro. A aritmética de expiração e epoch é verificada; o esgotamento do epoch
falha fechado em vez de reutilizar um fencing token.

`InMemoryControlPlane` usa por padrão 65.536 registros/slots de lease combinados
e orçamento estimado de 16 MiB retidos. `with_limits` pode reduzir ambos;
`stats` expõe bytes atuais/de pico, contagens e admissões rejeitadas. A rejeição
é atômica: um registro existente continua utilizável. O estado em arquivo mantém
a fronteira JSON V1 de 16 MiB e também limita o estado decodificado a 262.144
registros e 64 MiB.

Seu JSON V1 é decodificado por reader limitado e escrito diretamente em arquivo
temporário exclusivo. Backup e restore copiam um stream limitado, validam a
geração staged exata, sincronizam e só então substituem o destino. Somente os
mapas decodificados mantêm o estado completo em memória; a persistência não
retém outro buffer JSON completo.

**Maturidade:** contratos e referências RC estáveis; operação do serviço
externo pertence ao deployment.

## Candidato de release — 2026-10-14

`1.0.4-rc.2`: alinhamento de dependências.


## Correção em fonte: restauro que preserva fencing

`FileControlPlane::restore_from` exige agora estado atual legível e os mesmos slots de leases no backup preparado (incluindo épocas retidas, holder e timestamps de renovação/expiração). Qualquer diferença devolve `ControlPlaneError::Conflict` antes da substituição. Corrupção atual devolve erro controlado, preservando os bytes. Cópia, validação e substituição ficam sob o lock exclusivo; um temporário rejeitado é removido. Presenças continuam restauráveis quando o fencing é idêntico.

Migração: JSON V1 e assinaturas Rust permanecem iguais, mas a recuperação fica intencionalmente mais restrita. Restaurar snapshot antigo após aquisição, renovação, expiração/reaquisição ou release deixa de ser permitido; estado atual corrupto também não pode ser reparado por esta API porque não é possível provar as épocas já usadas. Preservar estado e backup para revisão. Nunca apagar/reinicializar o diretório nem regressar ao comportamento permissivo para contornar a rejeição. Perda total do estado do árbitro ou rollback externo dos ficheiros exige procedimento separado de recuperação/domínio, não resolvido aqui. Correção local ainda não publicada; consumo exige nova versão imutável. Não acrescenta autenticação Gateway, promoção distribuída nem proteção contra restauro fora da API.

Qualificação: 45 testes focados do crate passaram em macOS, incluindo reprodução antes falhada de rollback de época, tombstones após release, renovação/holder na mesma época, restauro de presenças com fences idênticos, preservação de corrupção, restart e corrida de handles independentes. Não qualifica duas máquinas físicas, DNET ou Windows.

O restauro retém temporariamente estado atual e preparado descodificados: cada um é admitido até 262144 registos e 64 MiB de payload retido estimado (128 MiB combinados), além de buffers limitados e overhead do alocador. Não retém cópia JSON codificada completa.


## Autoridade exata por serviço — fonte não publicada

`ServiceLeaseAuthority` é um contrato aditivo em `appcore-distributed-contracts::control_plane::lease_authority`. Aquisição, renovação exata, libertação exata, inspeção e check usam relógio servidor. `FileControlPlane` implementa TTL de 1–30000 ms: adquirir para o mesmo Core devolve a concessão viva sem prolongá-la; renovar não readquire uma concessão expirada e versões alteradas são recusadas. Escritor de dados e coordenador de updates usam `ServiceId`s distintos escolhidos pelo deployment. Heartbeat, papel e épocas HA de instâncias Gateway não conferem autoridade de escrita de dados.

No restart usar `FileControlPlane::open_existing_with_clock`: recusa estado ausente/corrupto e nunca inicializa novo domínio de épocas. Provisionamento inicial por `with_clock` é uma operação explícita. As operações de autoridade usam lock OS sem espera, reportando contenção/indisponibilidade. `with_fenced_service_write` verifica a concessão exata e executa um commit local confiável e limitado sob o mesmo lock; takeover não se sobrepõe ao commit mesmo se a concessão expirar durante ele. O callback não pode reentrar no árbitro; o host possui limites de I/O e idempotência da aplicação. Um check remoto é apenas observação: o storage remoto deve impor fencing atomicamente no commit. Não guardar um check bem-sucedido como permissão offline.

Gateway acrescenta composição explícita `make_service_lease_router(gateway, authority, replay)`, ausente do router default. POST `/v1/gateway/service-lease` recebe `ServiceLeaseRequestV1` (version=1, identity, service_id, action); ações acquire/renew/release/inspect/check. Autenticação obrigatória também em insecure-testing. Domínio isolado `service_lease_token_claims` exige nome command/query exato, subject Core, hash do pedido completo e JTI de uso único até 96 bytes. A credencial não pode durar mais do que a aquisição pedida ou, nas ações com grant, que a expiração da concessão. Tokens relay, alteração de scope/ação/época e coringas não autorizam. O emissor deve autorizar tenant/cluster/Core/serviço/ação antes de assinar; clientes não recebem o segredo de assinatura.

Compor `PeerNonceStore` durável dedicado e partilhado entre restarts/réplicas do árbitro, por exemplo `FilePeerNonceStore` em storage certificado. Não há default inferido. Pedidos até 16 KiB, oito operações admitidas sem fila de admissão. Autenticação/replay I/O e chamadas do árbitro executam em workers bloqueantes fora da thread de polling async. Deployment possui deadlines I/O dos providers, drenagem, TLS, relógio partilhado e custódia dos estados. Respostas: 401 sem credencial, 403 inválida/repetida, 409 concessão alterada/conflito e 503 autoridade indisponível/em contenção ou saturação. Rejeição nunca vira sucesso offline. Falha do provider ou resposta perdida é ambígua: consultar com nova credencial, sem adivinhar sucesso nem repetir uma escrita com nova época automaticamente.

Limites: Core identifica o escritor, não o PID; partilhar credenciais Core partilha autoridade. Provisionar Cores distintos para escritores concorrentes e serviço lógico estável para descoberta. Referência não é consenso, árbitro HA com discos independentes nem fencing de storage remoto. Perda/rollback total de estado do árbitro ou replay requer recuperação controlada/novo domínio; apagar ficheiros ou usar o construtor inicializador não é atalho de recuperação. Wire/storage V1 e APIs anteriores mantidos. Integração no consumidor, publicação registry e qualificação DNET/partição física das duas máquinas continuam pendentes.
