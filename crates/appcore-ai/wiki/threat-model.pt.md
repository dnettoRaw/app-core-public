# Threat model de IA

[English](threat-model.en.md) | [Français](threat-model.fr.md) |
[Guia](guide.pt.md) | [LLMs generativos](generative-llm.pt.md)

Escopo: `appcore-ai 0.1.0-beta.2`, backends opcionais Candle e
OpenAI-compatible, componente opt-in de `appcore-bin` e fronteiras Swarm
experimentais. A crate não afirma sandbox de processo nem zero trust.

| Ameaça | Controle | Limitação residual |
|---|---|---|
| modelo malicioso, trocado ou envenenado | tamanho exato + SHA-256; provenance opcional; bytes verificados antes da ativação | a policy precisa escolher publishers confiáveis |
| path traversal/symlink | nome por digest, root canônico, open no-follow, validação de metadata/tamanho do handle, temporário exclusivo e ativação atômica sem substituição | administrador local mantém autoridade sobre o host |
| decompression bomb/tensor excessivo | formato nativo sem compressão; limites de artefato, dimensões, classes, inputs, outputs, RAM/VRAM | formatos externos precisam de parser seguro do backend |
| metadata falsa/custom ops | validação de registry e `ModelSecurityPolicy`; formatos provider negados por default | habilitar formato arbitrário pode executar código do backend |
| vazamento de prompt/credencial | `Debug` redigido, observação sem payload e somente secret references | callback da aplicação ainda pode logar dados incorretamente |
| isolamento tenant | contexto autenticado, grants remotos exatos e validação de tenant na bridge | adapter do host faz parte da base confiável |
| crash de backend nativo | feature opcional, input limitado e tradução de erros | Candle roda no processo; não há crash sandbox |
| abuso/controle pelo hardware probe | queries limitadas e read-only de SO/sysfs/NVML, sem shell/WMI nem API de escrita de fan/clock/voltagem/potência | administrador local e kernel/driver são confiáveis; diagnóstico revela capacidade agregada |
| exaustão/DoS | probe single-flight fora do lock, governor, admission, deadline, cancelamento e limites fixos de filas/batches/registries/rotas/residents/peers/transfers | `Unrestricted` reduz headroom voluntário por definição |
| poisoning de training | dataset explícito limitado, seed reprodutível, resume/checkpoint verificados | o Runtime não conhece a verdade/qualidade do dataset |
| peer malicioso/discovery comprometido | authenticator AppCore, anúncios expirantes estritamente mais novos, rejeição de claims duplicadas, grants tenant e limites | load/performance anunciados podem ser falsos |
| poisoning ou withholding de artefato | digest/tamanho/provenance end-to-end, timeout e stores alternativos limitados | disponibilidade maliciosa consome o retry budget limitado |
| replay de execução | contrato da bridge exige replay protection, expiração e nonce do Peer RPC AppCore | a crate não cria segundo replay store |
| fake availability/churn | lease, health/custo e failover limitado | trabalho em execução pode falhar e só repete quando policy permite |
| resultado remoto não confiável | target autenticado, response limitado e diagnóstico explícito | correção genérica da inferência não é provada criptograficamente |

## Ameaças adicionais de LLM generativo

| Ameaça | Controle implementado ou exigido no deployment | Limitação residual |
|---|---|---|
| model server exposto | bind loopback, autenticação de host e firewall de deployment | administrador local controla o processo |
| prompt injection aciona tool | tools e autorização pertencem à aplicação; output nunca vira comando automaticamente | conteúdo não confiável continua influenciando o modelo |
| chat template/tokenizer trocado | binding exato, digest/revisão e digest por range do bundle | HTTP genérico não prova os bytes carregados pelo servidor externo |
| context/KV-cache DoS | tokens, contexto, sequences, queue e memória limitados antes do dispatch | tokenização real só é conhecida pelo engine |
| opção ignorada pelo engine | capability negotiation e erro explícito para sampling/tool não suportado | versões OpenAI-compatible não são semanticamente idênticas |
| output parcial após cancelamento | streaming opt-in checa cancelamento entre chunks limitados e aplica backpressure síncrono; respostas completas continuam limitadas | output já entregue não pode ser revogado; a aplicação marca o stream cancelado como incompleto |
| erro do provider vaza dados privados | somente status exato e `Retry-After` limitado em segundos atravessam a fronteira | bodies do provider continuam indisponíveis mesmo quando ajudariam no diagnóstico |
| JSON específico do provider substitui policy central | parâmetros validados rejeitam chaves reservadas, profundidade/nodes excessivos e controles | o owner do deployment deve testar a semântica do provider |
| engine nativo comprometido | processo isolado, path imutável, user sem privilégio e Supervisor | sandbox forte é responsabilidade da deployment |
| range de modelo segmentado corrompido | bundle ligado à identidade completa, ranges limitados sem sobreposição e SHA-256 por segmento | NVMe/local admin permanece na base confiável |

O servidor LLM nunca recebe acesso direto a filesystem tools por default. A
compatibilidade HTTP OpenAI é transporte, não boundary de segurança nem prova
de equivalência de sampling.

Invariantes: privacidade e resource mode locais vencem demanda remota; nenhum
peer força `Unrestricted`; contribuição nunca aumenta remotamente; modelos não
trafegam no RPC genérico; telemetria não inclui prompt/output/token/secret/URL;
artefato assinado falha fechado; capacidade desconhecida não vira ilimitada.

O fixture corrompido está em
[`tests/fixtures/corrupt-native-linear-v1.artifact`](../tests/fixtures/corrupt-native-linear-v1.artifact).
Byte sweeps e três targets `cargo-fuzz` exercitam o parser nativo, fronteiras de
contrato e decoder OpenAI-compatible limitado. Testes também disputam 32
writers pelo mesmo artefato e rejeitam symlink Unix em leitura full/range e
consulta de existência.
