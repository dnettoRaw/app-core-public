# Guia do appcore-filemaker-ai

Erros de conversão JSON retêm até 512 bytes sem cortar caracteres UTF-8 nem
manter um buffer de mensagem grande demais. Argumentos Unicode inválidos
retornam erro controlado e preservam documento e revisão; a call rejeitada
consome seu budget. Serde pode ter alocado a mensagem completa antes; isso não
é garantia de pico de alocação nem de redaction de segredos.

`filemaker_validate` também conta o envelope completo emprestado antes de montar
JSON, incluindo mensagens, escapes do template e truncamento. Warnings continuam
válidos se não houver erros ou truncamento; relatório truncado nunca vira
`valid: true`. O relatório do core ainda é construído antes, dentro do limite de
issues: isso evita a árvore JSON rejeitada, não o próprio relatório diagnóstico.

A admissão de nomes de tools e a validação da lista permitida usam contratos
estáticos de argumentos, sem reconstruir todos os schemas JSON públicos nem
reter um cache global. Quando solicitado para descoberta pelo modelo,
`tool_definitions()` copia nomes, descrições e schemas serializados exatos dos
contratos estáticos; não monta mais árvores JSON nem vetores intermediários por
categoria. Nomes desconhecidos falham antes
da contagem de calls; tools conhecidas mantêm os checks de policy, argumentos e budget.

`filemaker_capabilities` conta uma visão emprestada do contexto do documento,
limites e policy antes de montar a árvore JSON. Contexto purpose/rules/editable/
locked grande demais é rejeitado sem clonar essas coleções para JSON. Respostas
aceitas preservam campos, ordem das listas e contagem exata dos bytes escapados;
uma sessão vazia continua retornando `document_context: null`. A árvore de saída
continua owned, e chamadas rejeitadas consomem o budget de calls. Isso não limita
memória residente da sessão, construção de diagnósticos nem scratch de exporters.

O benchmark `runtime` inclui `create_patch_256_elements`: uma sessão nova cria
um Canvas de 256 retângulos, oculta um elemento por tool e inspeciona outro,
verificando revisões e resultados. Compile/bind e criação do JSON fixture ficam
fora do tempo; parsing JSON, conversão tipada, dois layouts, checks de policy/
resultado e destruição da sessão ficam dentro. Mede edição, não shaping de
texto, exporters, latência de cancelamento ou todas as tools.

Argumentos tipados de mutação, elementos source, comprimentos e overrides de
estilo são desserializados da árvore JSON existente, sem cloná-la primeiro.
O IR/patch resultante ainda possui suas strings e coleções; isso não significa
parsing sem cópias do texto JSON original.

Respostas de mutação são dimensionadas antes do commit de documento/cena.
Se `max_result_bytes` não comportar a resposta, create/load/patch e ferramentas
de edição derivadas retornam erro de policy sem mudar documento ou revisão.
A tentativa consome o budget de calls; a validação do candidato pode já ter
ocorrido. Esse budget de resposta não reserva memória temporária do layout.
O teto cobre o `ToolExecution` serializado inteiro, incluindo `tool`, `revision`
e `value`. Builders recebem somente o budget exato restante do value, então uma
rejeição do envelope externo não ocorre depois do commit de uma mutação.

`export_dataset_csv_controlled` aceita `OperationControl`, verifica cancelamento
antes da saída e nas fronteiras de linhas, e reporta linhas concluídas na fase
Export. Retorna `Cancelled` ao cancelar; o writer pode conter um prefixo CSV
parcial que o chamador deve descartar ou reverter. Callbacks de dataset/writer
são cooperativos, não preemptíveis. A bridge AI usa o mesmo controle para CSV.

Use `FileMakerAiSession::with_control(OperationControl)` para compartilhar
cancelamento e progresso com layout/reflow, validação/preflight e export gráfico.
Instale em `empty(...)` antes de create/load para controlar o layout inicial;
`new(...)` valida antes de uma chamada posterior ao builder. Chamadas canceladas
consomem o budget de calls. Um candidato cancelado no layout restaura cena e
revisão anteriores. Trocar controles não reinicia policies nem budgets.
Consultas de regiões livres usam o mesmo controle e checkpoints após cada
subtração de retângulos, na fase Preflight. Validação da cena e filtragem/
ordenação final não são interrompíveis internamente. O parsing de argumentos
verifica cancelamento antes, durante uma passagem sem alocação em blocos de
16 KiB e depois do Serde. A chamada Serde limitada ainda é indivisível e pode
consumir até o teto configurado de 1 MiB antes do check posterior. Conversão
de resultados e outros diagnósticos ainda têm lacunas de cancelamento;
callbacks/observers devem retornar prontamente e não são interrompidos à força.

Preview/export (incluindo CSV) transmitem os bytes do exporter por scratch
base64 de 8 KiB para a String limitada do resultado, retendo no máximo dois
bytes brutos entre writes em vez do artefato bruto completo. Metadados, IDs de
tabela, escapes JSON e loss reports usam o mesmo orçamento, reconferido no
envelope completo. Isso não limita o scratch do exporter ou codec.

Resultados tipados de inspect/explain, preflight, debug-mask e regiões livres
são contados contra `max_result_bytes` antes da conversão para Value JSON.
Resultados excessivos param na contagem, sem construir a árvore JSON adicional.
A checagem final permanece ativa. Isso não limita a cena resolvida ou os DTOs
já construídos nem substitui a contabilização de artefatos/envelope base64.
A inspeção de página é serializada de uma visão emprestada da cena: nomes de
exclusions/regions e IDs em overflow são percorridos diretamente, portanto a
rejeição não clona primeiro essas listas em `PageInspection`. A saída aceita
mantém exatamente o formato JSON do core e possui as strings JSON finais.

Este crate opcional adapta sessões determinísticas do `appcore-filemaker` aos
contratos de tools limitados aceitos pelo `appcore-ai`. Ele não adiciona
comportamento de IA ao compiler nem permite que um modelo escolha output no
filesystem.

Crie `FileMakerAiSession` com `ResourceLimits`, fonts, assets opcionais e
`AiBridgePolicy` explícitos. A policy limita chamadas, bytes dos argumentos
JSON, operações de patch e bytes do resultado serializado. As listas
`ai.editable` e `ai.locked` do template são aplicadas em toda subtree destrutiva
antes de um patch atômico alterar o documento. Purpose/rules textuais formam
contexto compacto para o modelo; o bridge determinístico não finge interpretar
regras em linguagem natural.
O dimensionamento do resultado serializa em um contador limitado que não retém
o payload e aborta assim que excederia `max_result_bytes`, evitando uma segunda
alocação JSON completa e preservando a fronteira exata em bytes.

Use `tool_definitions()` em `AiGenerationOptions` e passe chamadas exatas para
`execute_call`. Tools de consulta são somente leitura. Tools de mutação só
incrementam a revision depois que uma cópia candidata limitada valida e, para
modelos gráficos, resolve com sucesso. A sequência do patch é exatamente a próxima revision e o limite
efetivo de operações não pode superar `ResourceLimits` do core. Export retorna
base64 limitado em memória.

`filemaker_export` aceita PDF, SVG, PNG, JPEG, HTML e CSV. CSV seleciona uma
tabela vinculada (ou exige o ID exato quando houver várias) e percorre as linhas
limitadas diretamente do IR de dataset. Sessões de dataset não inventam uma
página; preview, masks, regiões livres e preflight gráfico ainda exigem uma
cena document/canvas.

Toda declaração de tool possui schema fechado igual aos argumentos aceitos;
campos desconhecidos falham. Capabilities expõem chamadas restantes e contexto
compacto do documento. `load` não pode substituir um documento confiável e sua
policy de IA sem opt-in do host em `allow_document_replacement`, falso por
default.

`filemaker_schema` relata cores tipadas e cada layer da cascata. A fronteira
limitada `filemaker_set`/patch aceita `set_style` transacional; overrides de
style no export são somente pintura e não alteram a geometria resolvida.

`filemaker_add` aceita o elemento de origem estrito e compacto quando o objeto
possui `type`, incluindo lengths de origem, paths semânticos, style, transform,
layer e colisão. Um `ElementIr` completo com `kind` continua aceito. O schema
anuncia unidades, primitivas, comandos de path e gráficos avançados preparados
para que o modelo não precise inventar operações de pintura em pixels.

`filemaker_inspect` aceita um ID de elemento ou uma página. Seu trace
estruturado e `filemaker_explain` preservam geometria de origem, anchors,
region, medição, colisão, página/reflow e provenance. `filemaker_debug_mask`
declara página e view collision/layout/visual/combined;
`filemaker_query_free_regions` declara suas dimensões mínimas limitadas.

Capabilities expõem PDF editável, flattened e híbrido e nomeiam as features PDF
preparadas restantes separadamente. Hybrid pinta outlines determinísticos e uma
camada Unicode invisível e subsetada para busca, seleção e extração. A
autodescrição de export garante writer do chamador ou bytes limitados,
loss report strict/best-effort, DPI somente raster, metadados PDF determinísticos
e subset de glyphs em PDF; o modelo não deve inferir output indisponível.

`filemaker_validate` retorna issues limitadas de layout e truncamento explícito.
`filemaker_preflight` declara formato/fidelity/modo/página/DPI, strict e policy
de acessibilidade no schema da tool. Discovery nomeia as etapas schema, dados,
layout e preflight, inputs completos do fingerprint e cache resolve-on-miss.

As tools de debug-mask e regiões livres passam os limites do core da sessão
para a geometria diagnóstica limitada. A execução da tool não pode contornar o
budget de comparações ou de geometria retida da cena.

A sessão confirma junto o documento imutável e sua cena resolvida. Tools de
leitura clonam apenas o `Arc` da cena; elas não refazem layout. Um patch monta e
valida um único candidato e então substitui os dois valores atomicamente; se a
edição falhar, documento e geometria anteriores permanecem válidos.
