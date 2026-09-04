# Guia do appcore-filemaker-ai

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
