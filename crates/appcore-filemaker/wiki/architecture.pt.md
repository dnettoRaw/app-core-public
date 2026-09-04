# Arquitetura

`appcore-filemaker` compila `Template + Dados + Patches` em uma IR tipada,
mede assets e fontes explícitas, resolve layout/colisão/reflow e produz uma
cena imutável. Inspeção, preflight e exporters consomem essa cena e não podem
alterar a geometria.

A geometria usa microunidades inteiras. O YAML aceita somente
`filemaker: "1.0"`. Includes, assets e fontes usam resolvers explícitos com
sandbox. `ResourceLimits` limita todos os parsers, loops, datasets, rasters e
saídas.

O binding usa um único contador de elementos compartilhado entre raízes,
descendentes e expansão de repeats, com cancelamento/progresso cooperativo nas
fronteiras de elemento. A busca de colisão tem um orçamento total próprio de
comparações além do limite de reflow, portanto cenas esparsas ou sobrepostas
adversariais falham fechadas em vez de executar trabalho quadrático ilimitado.
Assets de filesystem são abertos a partir de uma raiz canônica sem seguir um
symlink/reparse point final substituído, lidos sob o limite de bytes e
revalidados no sandbox ao redor da leitura. O cancelamento de export é
verificado antes de qualquer byte visível no `Write` do chamador.

O core nunca depende de `appcore-ai`. O bridge opcional traduz 20 tools
limitadas para operações do core; a CLI usa `appcore-args` e gravação atômica.
O contrato cobre temas/tokens explícitos, dados computados, paths semânticos,
caixas de página e intenção de imagem. A cena resolvida mantém glyphs, comandos
de path, placement da imagem, bounds distintos, provenance e metadados.

Canvas é um contrato de desenho semântico, não um buffer de pixels.
Coordenadas aceitam `pt`, `px`, `mm`, `cm`, `in`, `%`, `lu` lógico e valores
`norm`/`normalized` limitados a `0..=1`. Nós text, image, line, rect, circle,
ellipse, polygon, path e group preservam sua identidade na IR e no layout;
comandos de path são move, line, curve cúbica e close. Circle rejeita eixos
resolvidos desiguais. Safe area, presets, layers/z-index, transforms e colisão
continuam entradas explícitas e ortogonais da mesma cena fixed-point.

As cores atravessam YAML e IR sem perder seu espaço. A origem pode usar nomes
estáveis, hex, `rgb`/`rgba`/`gray` inteiros, `cmyk` em milionésimos ou um valor
tipado com tag. Fill é a pintura de fundo, stroke com sua largura é a borda, e
opacity permanece separado. Resolvers em memória e filesystem com raiz
canônica implementam as fronteiras de asset, template e font com bloqueio de
traversal e limites de bytes do chamador; registrar uma font nunca varre o SO.

A ordem normativa de style é executável: defaults do engine, theme ativo,
template, style expandido de component/nome/inline, `style_rules` condicionais
ordenadas no binding, `SetStyle` runtime transacional e, por fim,
`ExportStyleOverride`. Mudanças runtime precedem medição. A layer de export é
somente pintura (fill, stroke, opacity e cor de texto), portanto um exporter
não altera métricas de font, bounds de stroke ou layout. Layer e z-index apenas
ordenam a lista imutável de pintura e nunca influenciam decisões de colisão.

Metadados de imagem são resolvidos uma vez para assets raster e SVG. Contain e
scale-down preservam aspecto por razões em microunidades fixed-point; fill,
none no tamanho intrínseco, crop, cover focal e orientação EXIF opcional geram
retângulos imutáveis de origem, destino e clip. O preflight calcula DPI raster
efetivo após o transform. SVG/HTML incorporam SVG; PDF/raster relatam a
rasterização SVG ainda não suportada em vez de perder o asset silenciosamente.

A política de colisão segue uma cascata determinística de documento para
página, região, grupo e elemento. O atalho booleano `collision: false` é
explícito, e o índice espacial recebe o bound medido selecionado — layout,
visual ou intrínseco — antes do reflow.

Transforms também são resolvidos antes da consulta espacial. Translação,
rotação em graus inteiros, escala fixed-point, flip/mirror e origins explícitas
compõem através de grupos. PDF, SVG, raster e HTML compartilham a mesma matriz
resolvida e seus bounds visuais e de colisão.

A intenção de layout de texto atravessa YAML, IR, medição e export sem reflow
no renderer. `text_options.overflow` aceita `wrap`, `shrink`, `ellipsis`,
`clip`, `expand` ou `error`, junto de `max_lines` limitado,
`min_font_size` absoluto e `line_height` fixed-point. A expansão ocorre antes
da consulta espacial; clipping vira geometria resolvida; SVG e HTML consomem
runs moldados e truncados em vez do literal original. `writing_mode: vertical`
molda colunas de cima para baixo que avançam da direita para a esquerda, e todos
os exporters gráficos consomem essas colunas e runs resolvidos. PDF e raster
usam os avanços dos glyphs diretamente. Emoji colorido continua uma perda
explícita até um exporter implementá-lo.

Constraints geométricas são resolvidas antes de medição e colisão.
`constraints` carrega mínimo, preferido, máximo e aspect ratio largura/altura em
milionésimos; `align_x` e `align_y` escolhem início, centro ou fim dentro da
página/região/grupo ativo. Anchors podem apontar para a borda de um elemento já
resolvido ou para uma guide nomeada com `guide:nome[+offset]`. Coordenadas,
ranges e ratios contraditórios falham explicitamente. Move em runtime limpa
anchors/alinhamento; resize em runtime limpa constraints de tamanho anteriores.

Containers de flow distribuem filhos de tamanho fixo com `start`, `center`,
`end`, `space_between`, `space_around` ou `space_evenly`. Distribuição diferente
de start exige tamanho primário explícito, preferido ou derivado de aspect para
cada filho visível. Overflow e ambiguidade auto-medida são erros tipados.

`exclusions` nomeadas no nível superior são retângulos relativos à página,
resolvidos em geometria fixed-point antes de posicionar elementos. Elas não são
pintadas, devem ficar dentro do trim box, repetem em cada página física e
inicializam o índice espacial como regra imóvel de prioridade máxima. Os campos
opcionais `group` e `collides_with` usam o mesmo contrato simétrico de colisão
dos elementos. As políticas push/error/next-page/shrink existentes continuam
responsáveis pelo reflow; instâncias repetidas compartilham o orçamento global
de geometria. Inspeção, máscaras de colisão e consultas de regiões livres
mantêm a exclusão resolvida, enquanto exporters não recebem node para pintar.

O source estrito de página aceita layers `master`, `first`, `continuation` e
`last`. Cada layer tem listas explícitas de elementos `background`, `header` e
`footer`. Elementos master repetem em todas as páginas; uma layer de papel semântico é
selecionada depois da paginação do corpo; e o texto `{page}`/`{pages}` só é
substituído quando o total limitado é conhecido. Elementos de layer compartilham
componentes, temas/estilos, binding, patches, medição e exporters com o corpo,
mas ficam em bandas de pintura sem colisão. Tabelas, repeat e anchors para outros
elementos são rejeitados ali para que decoração não repagine o corpo.
O flag resolvido `collidable` mantém esses overlays fora do preflight de
colisão, das máscaras de colisão e da subtração de regiões livres sem remover
sua pintura.

O motor de tabela consome streams `Dataset` reiniciáveis sem materializar toda
a entrada. Colunas fixed, auto com amostra limitada e flex ponderada viram
larguras exatas. Alturas fixas ou medidas por callback paginam com capacidade
correta de header inicial/repetido, limites de grupo, estilos condicionais em
ordem e totais integer/decimal/currency verificados apenas na página final.
Linhas, fields, bytes por célula, steps, amostras e páginas têm limites explícitos.

O frontend YAML estrito aceita intenção de tabela somente em elementos
`type: table` e exige binding para um array. Colunas, agrupamento, totais,
estilos condicionais, política de header e tamanho de linha atravessam para
`TableIr`; o binding valida linhas object e preserva valores tipados. Limites do
template só podem restringir os limites globais de linhas, fields e células.

O layout consome essa intenção tipada e emite um `ResolvedTableFragment` por
página física da cena. Larguras finais, repetição de header, retângulos de linha
e célula, estilos por regra de dados, continuidade de grupo, geometria de totais
e texto moldado das células tornam-se input imutável dos exporters. Uma
continuação participa dos limites e da colisão normais; renderers nunca medem
nem repaginam a tabela.

PDF editável/flattened/híbrido, SVG, raster e HTML agora pintam esses fragments
diretamente. O uso de fonts no PDF inclui todos os runs de célula; SVG/HTML
incorporam fonts selecionadas por estilos de dados; raster contorna os mesmos
glyphs. HTML semântico preserva tabela, header, body, linha, grupo e footer; o
modo fixed usa as mesmas dimensões resolvidas. Preflight valida quantidades de
células, bounds, diagnósticos e disponibilidade de font incorporada para os
modos PDF editável e híbrido.

Capacidades preparadas são explícitas: fidelidade estrita retorna
`FM-EXPORT-UNSUPPORTED`; best effort registra a perda exata. Nenhum renderer
faz aproximação silenciosa.

Debug só é derivado depois do layout. `DebugOverlay` suporta grids exatos de
1/5/10/20 pontos, rulers, coordenadas, IDs, bounds distintos, anchors, regions
resolvidas, safe area, geometria de colisão/exclusão e crosshairs, sem entrar na
lista de pintura da cena. Masks collision/layout/visual/combined derivam seus
próprios retângulos ocupados e livres e exportam PNG, PDF, SVG ou JSON estável
com occupied/free/collisions/overflow. Cada elemento resolvido mantém um trace
limitado de x/y/width/height de origem, anchors, region, geometria proposta,
medição, policy de colisão herdada, página/reflow e provenance para inspeção
estruturada e explicações determinísticas.
Exports JSON, SVG e PDF da mask primeiro contam sob `max_output_bytes` sem reter
o output e depois serializam direto no writer do chamador. PDF usa o emissor
compartilhado de objetos/xref e um stream de comandos de tamanho exato, não um
buffer de página ou arquivo. Isso preserva a rejeição antes da escrita; a API
JSON de conveniência dimensiona previamente sua única alocação exata.

Opções de export são específicas do formato. DPI afeta somente PNG/JPEG e
qualidade JPEG somente JPEG. PNG começa transparente e preserva alpha; JPEG
compõe sobre branco somente depois de registrar perda de alpha do style ou da
imagem raster. HTML anuncia capacidade semântica somente no modo semantic. PDF
editable, flattened e híbrido compartilham metadados determinísticos de
title/creator/producer. Editable incorpora subsets exatos de glyphs e mapas
Unicode. Hybrid pinta os mesmos outlines determinísticos de flattened e depois
posiciona texto Unicode invisível e subsetado nas coordenadas resolvidas dos
glyphs para busca, seleção e extração. Todo formato de documento grava em `Write` do chamador ou
`export_bytes` limitado; CSV transmite linhas e também oferece bytes limitados.
Links, bookmarks, acessibilidade tagged, PDF/A, WebP, XLSX, ZPL e
ESC/POS continuam contratos preparados nomeados, sem aproximação silenciosa.

A validação tem quatro fronteiras explícitas: schema, dados tipados e bindings,
layout resolvido e então preflight consciente do exporter. Reports preservam
warnings limitados; policy strict rejeita warnings e truncamento sempre falha
fechado. O preflight antecipa gaps de asset/vector, CMYK, alpha JPEG, DPI
efetivo, embedding de fonts e acessibilidade, além de glyphs, overflow e colisão.

O fingerprint determinístico enquadra versões de schema/engine,
template/dados/patches canônicos, digests dos assets referenciados e das fonts
registradas. Campos JSON canônicos passam por um writer de dimensionamento
e então direto pelo SHA-256 sob o budget agregado `max_output_bytes`, preservando
o framing V1 sem um buffer JSON completo. `LayoutEngine::resolve_cached` resolve somente em miss do
`SceneCache` limitado, compartilha cenas imutáveis para render-many e rejeita
versões antigas do engine.
O cache é limitado pela quantidade de entradas e pelos bytes serializados
agregados das cenas. `OperationLog::new_bounded` aplica o mesmo limite duplo a
snapshots `Arc<DocumentIr>`; undo e redo movem os documentos em vez de cloná-los
novamente. `BorrowedDataset` percorre uma slice de linhas existente sem duplicá-la.

Somente a página limitada atual da tabela retém cópias das linhas de origem. O
sink de layout converte essa página imediatamente em `ResolvedTableFragment`,
sem acumular páginas cruas ao lado da cena resolvida. CSV empresta células
textuais quando possível e escreve escapes de aspas em partes.

A composição raster usa faixas verticais de no máximo 256 linhas e cerca de
4 MiB, com teto separado de 4 MiB por scanline. PNG transmite as faixas em
ordem; JPEG as solicita pelo percurso documentado de blocos 8x8, e máscaras de
colisão PNG reutilizam o encoder por faixas. O renderer continua consumindo
somente geometria resolvida, sem medir ou resolver colisão.
CSV transmite linhas. SVG e HTML fazem uma passagem limitada de contagem e
depois escrevem markup, texto escapado, paths e assets base64
incrementalmente, preservando a rejeição por limite antes de tocar no writer.
PDF aplica o mesmo padrão de dimensionamento limitado, emite um chunk de objeto
independente de `pdf-writer` por vez, rastreia offsets e escreve xref/trailer no
fim. Não retém um buffer final do documento; imagens decodificadas e subsets de
fonts são liberados progressivamente.
O SVG da máscara de colisão segue esse caminho de contagem/streaming e escapa
IDs em partes; o JSON da máscara dimensiona pretty-JSON determinístico e então
serializa diretamente. O workload `collision_mask_json_4m` mede uma máscara de
4.188.826 bytes com checkpoints RSS idle, pico e retido.
O PDF da máscara grava comandos fixed-point direto no stream declarado e então
conclui o xref clássico. `collision_mask_pdf_100k` mede 100.000 retângulos e um
PDF exato de 1.800.626 bytes sob os mesmos checkpoints.

Os gates de confiabilidade mantêm snapshots exatos do SVG visual e da mask de
colisão, além de properties da geometria fixed-point. Fuzz targets separados
exercitam o pipeline YAML/bind/layout limitado, dados Unicode arbitrários e
texto enorme, assets raster corrompidos, geometria absurda e overlaps, anchors
circulares e grafos de include malformados, circulares ou profundos demais;
input malformado pode retornar erro tipado, mas não pode causar panic, loop
infinito ou alocação sem limite.

A fronteira final de cena pública é protegida independentemente da compilação.
Export e preflight rejeitam versão antiga do engine, styles ou placements de
imagem malformados, overflow de coordenadas e excesso dos budgets de
páginas/elementos/paths/linhas/texto antes de escrever. APIs limitadas de
overlay, derivação/JSON de mask, colisão e regiões livres consomem
`max_preflight_comparisons`; os atalhos usam defaults limitados. `ElementId`
valida novamente durante a desserialização.
Os checkpoints do export controlado executam dentro dos loops reais de elementos
do renderer; o cancelamento ainda impede que o artifact preparado chegue ao
chamador.
O pipeline de fonts explícitas usa shaping mantido por `harfrust` e validação,
métricas e outlines por `skrifa`; a auditoria final removeu as dependências sem
manutenção `rustybuzz` e `ttf-parser` sem habilitar descoberta no sistema.
Quando uma font válida omite capital height em OS/2, a policy nomeada do
descriptor PDF usa ascent como `CapHeight`; advances ausentes continuam erros
tipados.

O benchmark runtime mantém a compilação focada separada do workflow A4
completo. `a4_report_end_to_end` e `a4_report_pdf_hybrid` decodificam o YAML e
os dados mantidos de duas páginas, aplicam patch transacional, resolvem
medição/colisão/reflow, executam preflight estrito e transmitem PDF editável ou
híbrido para um sink. `a4_report_export_matrix` reutiliza esse pipeline completo
uma vez por iteração e cobre nove outputs: PDF editável, flattened e híbrido;
SVG; HTML semântico e fixo; PNG; JPEG com losses best-effort explícitos; e CSV
do dataset. No Apple M1 ele mediu 70,56 ms p50, 71,34 ms p95, MAD de 0,22 ms e
10,64 MiB de RSS pico. `appcore-dev bench` coleta cada workload em processos
isolados, portanto seu pico de RSS não é confundido com o caso menor de
compilação do Canvas.
