# Guia do appcore-filemaker

Mensagens diagnósticas são cortadas somente em fronteiras UTF-8: erros retêm
até 1.024 bytes; paths de origem, mensagens de validation issues e perdas de
export retêm até 512. Buffers grandes são substituídos pelo prefixo limitado.
Isso limita o texto retido, não a alocação anterior do chamador, o overhead do
allocator nem a quantidade de relatórios acumulados.

Execute `cargo test -p appcore-filemaker --test reflow` para verificar o limite
exato de tentativas de push, rejeição de gap negativo e parada de shrink no
tamanho mínimo. Erro de limite não prova execução do detector de estado repetido.

`reflow_dense_64` resolve 64 retângulos inicialmente sobrepostos com limite de
64 tentativas e verifica cada posição final. `reflow_limit_63` usa a mesma
fixture e exige erro explícito de limite com 63 tentativas. Compilação e binding
ficam fora do timer; criação do engine, layout/reflow, verificações e descarte
entram na medição. Os casos não isolam measurement de texto ou custo de busca
de colisões e não exercitam um ciclo geométrico.

O benchmark `diagnostic_geometry_256` deriva uma máscara Combined e consulta
regiões livres numa grade resolvida de 16 por 16 retângulos. Verifica remoção
de duplicatas, ausência de colisões/overflow e resultados livres idênticos.
Compile/bind/layout preparam a fixture fora do timer; validação, derivação,
consulta, verificações e descarte dos resultados entram na medição. Não mede
reflow denso, measurement de texto nem encoding dos exporters.

A subtração diagnóstica de retângulos mantém até quatro partes temporárias
inline, sem alocar um `Vec` por comparação bem-sucedida. Consultas de regiões
livres e máscaras de debug compartilham o helper, preservando ordem e budgets.
As listas de regiões retidas ainda alocam; isso não é um limite de memória do
processo nem uma medição de redução de RSS ou ganho de velocidade.

Seleção de bounds de debug e deduplicação por elemento também usam quatro
slots fixos. Máscaras Combined removem bounds duplicados por elemento;
overlays mantêm cada classe selecionada na ordem original.

`SceneInspector::query_free_regions_controlled` aceita `OperationControl`.
Verifica cancelamento antes/depois da validação da cena e da filtragem/ordenação
final, e informa subtrações de retângulos concluídas na fase Preflight. Essas
passagens de validação/ordenação não são interrompíveis internamente. A consulta
subtrai bounds de colisão e exclusões resolvidos, respeita budgets diagnósticos
e nunca lê uma máscara de debug nem altera a cena. Cancelamento descarta o
resultado parcial; observers síncronos devem retornar prontamente. Os métodos
anteriores mantêm seu comportamento sem alocar um token de controle.

`export_dataset_csv_controlled` aceita `OperationControl`, verifica cancelamento
antes da saída e nas fronteiras de linhas, e reporta linhas concluídas na fase
Export. Retorna `Cancelled` ao cancelar; o writer pode conter um prefixo CSV
parcial que o chamador deve descartar ou reverter. Callbacks de dataset/writer
são cooperativos, não preemptíveis. A bridge AI usa o mesmo controle para CSV.

A contagem do cache para assim que os bytes serializados excedem o orçamento;
não codifica nem percorre o restante da cena apenas para rejeitá-la.

Num miss, capacidade de entradas/bytes totalmente ocupada por consumidores
é rejeitada antes de chamar o resolver, sem evictar entradas. Hits continuam
disponíveis. A pré-checagem é conservadora sob liberação concorrente de leases
e não reserva scratch; a inserção final ainda valida a admissão.

A admissão no SceneCache contabiliza cenas no cache e cenas removidas ainda
retidas por consumidores. `used_bytes()` informa bytes serializados no cache;
`retired_bytes()` informa bytes observados das cenas removidas ainda vivas.
Limites de entradas e bytes podem rejeitar inserção enquanto um Arc anterior
estiver vivo. Evicções FIFO podem ocorrer antes da rejeição; solte handles
antigos antes de tentar novamente. Referências fracas não mantêm cenas vivas e
seu número é limitado pela capacidade. Não é orçamento de heap/RSS: scratch
de compilação, cópias e alocações de Arc::make_mut ficam fora desse controle.

Comece pelo
[guia YAML passo a passo](https://wiki.appcore.dnettoraw.com/pt/crates/appcore-filemaker-yaml).
Ele constrói um template V1 estrito de forma incremental e traz a referência
completa dos campos aceitos no topo e nos elementos. Mantenha
`appcore-filemaker schema --json` como fonte executável da versão instalada.

Depois compare o [exemplo básico](examples/basic.pt.md) e o
[exemplo intermediário](examples/intermediate.pt.md). A
[referência de arquitetura e contratos](architecture.pt.md) explica os limites
do engine.

As camadas de página são percorridas de forma lazy em cada página física; a
resolução por role não cria uma lista temporária de referências de elementos.
O planejamento de fluxo distribuído usa a mesma passagem sem alocação para
calcular o espaçamento dos filhos visíveis.
O fingerprint também ordena nomes de assets emprestados, sem clonar cada nome
na resolução determinística.

Registre bytes exatos de fontes ou uma face PDF Standard explicitamente, além
de uma lista ordenada de fallback antes da medição; a ordem entra no fingerprint
e exporters incorporam as famílias com contornos escolhidas nos glyph runs.
Aplique patches de runtime no
binding, antes do layout, para que medição, colisão, paginação e export usem
geometria recalculada.
O JSON do fingerprint usa uma passagem de dimensionamento seguida de hashing
direto sob o budget agregado `max_output_bytes`. Ele preserva o framing V1
exato sem reter os bytes JSON canônicos.
Templates PDF também podem registrar explicitamente uma das doze faces latinas
PDF Standard 14 com `FontManager::register_pdf_standard`. Larguras e kerning AFM
participam da quebra e do alinhamento; o PDF editável referencia a face Type 1
sem busca no host nem embedding. A opção é somente PDF, usa WinAnsi e rejeita
texto não representável, salvo se um fallback explícito o cobrir. SVG, HTML,
raster e PDF flatten exigem contornos e não substituem uma fonte do sistema.
Symbol e ZapfDingbats ainda não são suportadas. Cada run Standard 14 é pintado
com uma operação nativa de texto PDF; o leitor aplica os avanços nativos da
string, enquanto as métricas AFM continuam orientando o layout.

Para japonês vertical ou layouts semelhantes, use
`text_options.writing_mode: vertical`. O engine quebra pelo limite de altura,
molda cada coluna de cima para baixo e avança as colunas da direita para a
esquerda. Mantenha `horizontal` (o padrão) para texto horizontal e BiDi.

O texto é limitado por `ResourceLimits::max_text_bytes` (4 MiB por padrão) e
por um teto absoluto de 4 MiB no motor; aumentar o limite configurado não eleva
esse teto. Texto acima do limite é rejeitado, nunca truncado. Texto expandido
fora de tabelas pode continuar entre páginas em limites de linhas moldadas.
Uma linha/célula de tabela é indivisível: precisa caber no corpo da página ou
o layout falha. Estilos mistos são compostos com elementos/linhas separados;
runs inline mistos num nó ou célula não são suportados pelo YAML.

Para colunas comerciais, use `text_options.align_x: end` num elemento de texto
ou `align_x: end` numa coluna da tabela. O alinhamento é aplicado a cada linha
depois de wrap e shrink, portanto valores com larguras de glyph diferentes
terminam na mesma borda da coluna. `start`, `center` e `end` são suportados.
Não use texto espaçador para alinhar valores; propriedades YAML desconhecidas
continuam sendo rejeitadas. Para compor uma linha de estilo compartilhado,
`text_segments` aceita partes literais ou vinculadas a strings e `gap_after`
opcional. Exija `text_options: { overflow: error, max_lines: 1 }`; larguras e
gaps são medidos juntos antes do alinhamento e preservados pelos exporters.
Segmentos não quebram independentemente. Conteúdo multilinha ou com estilos
diferentes usa elementos de fluxo separados. Use `style.underline: true` para desenhar cada linha
horizontal moldada, inclusive em células com estilo condicional. O sublinhado
acompanha a largura medida dos glyphs, não altera o layout e é omitido no texto
vertical.
Regras condicionais da tabela também podem usar `text_offset_y` para deslocar
verticalmente o texto desenhado sem alterar medição, altura das linhas ou
paginação. Use um comprimento absoluto ou lógico não zero, menor que o tamanho
efetivo da fonte e a altura interna da célula. O texto permanece recortado aos
limites internos originais; PDF, SVG, raster e HTML aplicam o deslocamento.
`text_offset_y_first_page` e `text_offset_y_continuation` podem sobrescrevê-lo
conforme o papel físico da página.
Use `text_options.padding_inline: 4pt` para padding simétrico no eixo inline.
O wrap considera a largura interna reduzida e mantém o recuo em todas as
linhas, inclusive nas células da tabela quando declarado no elemento.
Para insets do bloco de texto nos dois eixos, declare
`text_options.padding: { top: 2pt, right: 4pt, bottom: 2pt, left: 4pt }`.
Os insets participam da medição e são compartilhados pelos exporters; a
paginação de texto expandido reserva o padding superior e inferior em cada
fragmento de página. Use valores absolutos, lógicos ou percentuais abaixo de
50%, sempre não negativos; `auto` é rejeitado. Percentuais horizontais usam a
largura do elemento e percentuais verticais usam a altura.
O recuo inicial do parágrafo também é preservado nas linhas de continuação após a quebra.
Regras condicionais de tabela podem declarar `padding` por lado para linhas
correspondentes; vence a última regra que declarar padding, somado aos insets da
coluna antes da medição, paginação e exportação.
Regras também podem usar `padding_first_page` e `padding_continuation` para
substituir insets por papel de página; quando omitidos, vale `padding`.
`style.line_height` pode substituir o line-height do texto em uma linha
condicional. É uma razão em milionésimos entre 500000 e 4000000; `1250000` é 1,25.
Padding por coluna usa `padding: { top: 1pt, right: 2pt, bottom: 1pt, left: 2pt }`.
São aceitos somente comprimentos absolutos ou lógicos não negativos. O padding
reduz os limites do conteúdo medido em cabeçalhos, linhas e totais e é aplicado
de forma consistente em todos os exporters.
Para bordas condicionais de células, combine `style.stroke` e `style.stroke_width`
com `style.stroke_sides: { top: true, right: false, bottom: true, left: false }`;
os lados selecionados usam a cor e a largura de traço da célula.

Num fluxo vertical, marque cada elemento relacionado até o penúltimo com
`keep_with_next: true`. Um bloco que cabe numa página passa inteiro para a
próxima se não couber no espaço restante. Um elemento mais alto que a página
falha antes de criar continuações vazias; linhas de um único texto ainda não
Texto horizontal com `overflow: expand` em fluxo vertical é dividido em limites
de linhas moldadas completas quando excede a área de conteúdo; uma linha maior
que essa área ainda falha. `group_by` apenas marca inícios de grupos.
Separadamente, `keep_together_by: layout_group` mantém linhas adjacentes com a
mesma chave não nula na página quando couberem; grupos maiores são divididos
entre linhas completas e cada linha precisa caber. A chave é obrigatória em
todas as linhas, mas não precisa ser uma coluna visível.
Para ancorar uma linha específica, declare `row_anchor_field: row_anchor`,
grave uma string única como `summary` nessa linha e use
`table-id::summary.bottom`. O elemento acompanha a linha até sua página física.
Use `collision: false` quando o alvo estiver intencionalmente dentro do retângulo
de layout reservado pela tabela.
Regras condicionais podem definir `reserve_after: 18pt` para linhas ancoradas
correspondentes. Esse comprimento absoluto positivo reserva capacidade de
paginação para o conteúdo seguinte sem alterar a geometria renderizada da linha;
a tabela precisa declarar `row_anchor_field`.
Quando a primeira página e as continuações têm áreas de tabela diferentes, use
`page_bodies.first` e `page_bodies.continuation`, cada uma com `offset_y` e
`height` relativos ao elemento da tabela. Os retângulos devem caber nesse
elemento; offsets não podem ser negativos e alturas devem ser positivas. A
paginação seleciona a área conforme a página física.
Estilos condicionais também podem definir `min_height` para linhas
correspondentes, ou `min_height_first_page` / `min_height_continuation` para
áreas por página. A altura final é o maior valor entre o conteúdo medido e os
mínimos aplicáveis, permitindo espaçamento explícito sem fonte exagerada.

## Responsabilidade pela memória raster

Os casos `raster_png_dense_rows_8` e `raster_png_dense_rows_256` renderizam
4.096 retângulos e um fundo. A fixture desativa colisão para isolar export,
não reflow. Quatro casos `raster_candidates_*` comparam seleção linear com
índice experimental por faixa de 8/256 linhas. O índice existe apenas no bench,
com teto de 65.536 associações; construção e descarte entram no timer. Listas
e ordem exatas são verificadas fora da medição; contagens/checksums dentro.
O modelo cobre uma página a 96 DPI e reproduz a margem de antialias, não um
índice de produção geral.

O benchmark runtime compara faixas PNG/JPEG de 8, 64 e 256 linhas na mesma
cena FHD resolvida (fundo e 256 retângulos), escrevendo em um sink. Casos:
`raster_png_rows_8/64/256` e `raster_jpeg_rows_8/64/256` (um sufixo numérico
por caso). Compile/bind/layout ficam fora do timer; validação de export,
rasterização, encoding e verificações entram. Mede o trade-off das faixas,
não uma implementação de índice de elementos nem o heap nativo.

Para PNG/JPEG, `export_raster_controlled` recebe `RasterOptions::new(bytes,
rows)` sem mudar `ExportRequest`. O padrão continua 4 MiB e 256 linhas; limites
aceitos vão de 1 byte a 64 MiB e de 1 a 4096 linhas, com teto separado de 4 MiB
por scanline. Uma scanline que não caiba é rejeitada antes da codificação.
Por exemplo, `RasterOptions::new(1024 * 1024, 64)?` limita a superfície a
1 MiB/64 linhas. Faixas menores podem repetir mais renderização, especialmente
na leitura em blocos JPEG. Formatos não raster são rejeitados; layout, perdas
e overrides de pintura seguem compartilhados. Exports existentes, CLI/AI e
máscaras usam o padrão. O teto não inclui codec/assets/output nem RSS global.

JPEG libera a faixa anterior antes de renderizar a substituta, evitando duas
superfícies simultâneas na troca do cache. Se a renderização falhar, o cache
fica vazio e o erro é preservado. PNG transmite faixas sem acumular a superfície
completa. Scratch do codec, assets decodificados e buffers do caller consomem
memória adicional; o teto da faixa não é um limite de RSS do processo.
Não foi medida redução de RSS para esta correção do tempo de vida do cache.

Encoders internos rejeitam dimensões e altura de faixa zero antes de escrever
ou chamar o renderer. O renderer verifica o teto planejado de linhas antes de
alocar. São fronteiras defensivas; a validação pública de exportação continua
ocorrendo antes dessa camada.

Depois de resolver uma cena, chame `audit_layout` com limites de recursos e
`LayoutSafetyOptions` explícitos. O relatório é limitado, expõe contagens de
overflow/colisão/texto e pode rejeitar avisos no modo estrito. Sua representação
JSON é estável para fixtures golden e evidências de suporte; o export continua
responsável pelo preflight específico do formato.
