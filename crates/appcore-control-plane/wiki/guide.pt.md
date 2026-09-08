# appcore-control-plane

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
