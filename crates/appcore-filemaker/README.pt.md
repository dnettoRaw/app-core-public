# appcore-filemaker

Mensagens diagnósticas são cortadas somente em fronteiras UTF-8: erros retêm
até 1.024 bytes; paths de origem, mensagens de validation issues e perdas de
export retêm até 512. Buffers grandes são substituídos pelo prefixo limitado.
Isso limita o texto retido, não a alocação anterior do chamador, o overhead do
allocator nem a quantidade de relatórios acumulados.

Para PNG/JPEG, `export_raster_controlled` recebe `RasterOptions::new(bytes,
rows)` sem mudar `ExportRequest`. O padrão continua 4 MiB e 256 linhas; limites
aceitos vão de 1 byte a 64 MiB e de 1 a 4096 linhas, com teto separado de 4 MiB
por scanline. Uma scanline que não caiba é rejeitada antes da codificação.
Por exemplo, `RasterOptions::new(1024 * 1024, 64)?` limita a superfície a
1 MiB/64 linhas. Faixas menores podem repetir mais renderização, especialmente
na leitura em blocos JPEG. Formatos não raster são rejeitados; layout, perdas
e overrides de pintura seguem compartilhados. Exports existentes, CLI/AI e
máscaras usam o padrão. O teto não inclui codec/assets/output nem RSS global.

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

## Contrato genérico de layout de documentos

Use `text_options.align_x: start|center|end` em texto e `align_x` na coluna da
tabela. O alinhamento usa a medida final da fonte e é aplicado por PDF, SVG,
raster e HTML. Referências ao ID de uma tabela paginada apontam para o último
fragmento. Cada coluna pode declarar `padding: { top, right, bottom, left }`
com valores absolutos ou lógicos não negativos. Os insets reduzem a área de
medição das células e são compartilhados pelos exporters; colunas `auto`
incluem os insets horizontais na largura medida. Regras condicionais também
podem declarar `padding` por lado para células das linhas correspondentes. Vence
a última regra correspondente que declara padding; seus insets somam-se aos da
coluna e participam da medição, paginação e exportação.
Para composições de uma linha com estilo compartilhado, `text_segments` aceita
partes literais ou vinculadas a strings, em ordem, com `gap_after` opcional.
Use `text_options: { overflow: error, max_lines: 1 }`; partes e gaps formam uma
linha medida e alinhável, preservada em PDF, SVG, raster e HTML. Segmentos não
quebram de forma independente; conteúdo multilinha ou com estilos mistos deve
usar elementos de fluxo separados.
`padding_first_page` e `padding_continuation` podem sobrescrever o padding da
regra condicional por papel de página; quando omitidos, vale `padding`.
`style.line_height` substitui o line-height compartilhado nos estilos
correspondentes, inclusive em linhas condicionais da tabela. É uma razão em
milionésimos entre 500000 e 4000000; por exemplo, `line_height: 1250000` é 1,25.
`keep_with_next: true` mantém um bloco contíguo
junto quando ele cabe numa página. Em fluxo vertical, texto horizontal com
`overflow: expand` é dividido entre linhas moldadas completas quando excede a
área de conteúdo; uma linha individual mais alta que essa área ainda falha.
Outros elementos maiores que a página não são divididos. `group_by` apenas
marca inícios de grupos da tabela. Use `keep_together_by: layout_group` para
manter linhas adjacentes com a mesma chave não nula na página quando couberem;
grupos maiores são divididos entre linhas e cada linha precisa caber.

Para uma linha específica, defina `table.row_anchor_field` com um campo de
metadados que contenha strings únicas e limitadas; use `table-id::nome.top` ou
`table-id::nome.bottom` no anchor. Valor ausente ou `null` não cria anchor;
nomes duplicados são rejeitados.
Regras condicionais podem definir `reserve_after: 18pt` para linhas ancoradas
correspondentes. Esse comprimento absoluto positivo reserva capacidade de
paginação para o conteúdo seguinte sem alterar a geometria da linha; a tabela
precisa declarar `row_anchor_field`.
O fixture executável `examples/row-anchor-reserve.yml` com
`examples/row-anchor-reserve-data.json` mostra a linha ancorada indo para uma
página de continuação junto com o elemento seguinte.

Para separar a área da tabela na primeira página daquela das continuações, use
`table.page_bodies.first` e `.continuation`, cada uma com `offset_y` e `height`
relativos ao elemento da tabela. Ambos os retângulos devem caber nesse elemento;
offsets negativos e alturas não positivas são rejeitados.
Estilos condicionais também aceitam `min_height` por linha correspondente; a
paginação usa o maior valor entre o conteúdo medido e o mínimo aplicável.
Use `min_height_first_page` / `min_height_continuation` quando o espaçamento
precisar variar entre a primeira página e as continuações.

Use `text_options.padding_inline: 4pt` para padding simétrico no eixo inline.
O wrap considera a largura interna reduzida e preserva o recuo em todas as
linhas, inclusive nas células que usam as opções de texto da tabela.
Elementos de texto também aceitam padding de bloco, por exemplo
`text_options.padding: { top: 2pt, right: 4pt, bottom: 2pt, left: 4pt }`.
Os insets reduzem os limites medidos e são preservados nos exports PDF, SVG,
raster e HTML; a paginação de texto expandido reserva os mesmos insets verticais
em cada fragmento. São aceitos comprimentos absolutos, lógicos e percentuais
abaixo de 50%, sempre não negativos (`auto` é rejeitado); percentuais horizontais
usam a largura do elemento e os verticais usam sua altura.
O recuo inicial do parágrafo também é preservado nas linhas de continuação após a quebra.

Literais Rust de `TextOptions` devem inicializar `align_x` e `padding_inline`;
`TextLayout.padding` usa `Insets::default()` e `TextSourceOptions.padding` usa
`TextBlockPadding::default()` para manter o comportamento anterior.
`TableColumn` deve inicializar `align_x`. No YAML de coluna, o padrão é `start`.
Literais de `TextLine` também inicializam `source_text`; cenas serializadas
antigas leem esse campo como vazio, mas a paginação exige layout recém-moldado.

`keep_with_next` aplica-se a elementos irmãos em fluxo vertical. `style.underline`
desenha um sublinhado por linha horizontal moldada, inclusive em estilos condicionais
de células; não altera a medição nem é desenhado em texto vertical. Configure
`style.stroke`, `style.stroke_width` e, opcionalmente,
`style.stroke_sides: { top: true, right: false, bottom: true, left: false }`
para selecionar os lados da borda da célula; lados omitidos ficam ativos.
Regras condicionais também aceitam `text_offset_y`, um deslocamento vertical
apenas visual. Ele não altera conteúdo medido, altura das linhas ou paginação e
é recortado aos limites internos originais em todos os exporters visuais.
Use `text_offset_y_first_page` ou `text_offset_y_continuation` para sobrescrevê-lo
conforme o papel físico da página.
Rich text, codificação EAN13 e composição arbitrária de grupos de valores não estão suportados.
O limite padrão de texto é 4 MiB; `losses=0` indica suporte do exporter,
não equivalência visual com outro renderer.

**Beta pré-1.0.** Valide outputs, limites e falhas para sua carga; os testes do
pacote não equivalem à certificação de produção.

[English](README.en.md) | [Français](README.fr.md)

Guia de migração: [English](wiki/migration.en.md) | [Português](wiki/migration.pt.md) | [Français](wiki/migration.fr.md)

Compilador determinístico do AppCore para documentos declarativos, canvases
vetoriais semânticos e datasets limitados. O YAML versionado
`filemaker: "1.0"` é apenas um frontend: compilação, binding de dados, layout,
colisão, inspeção, preflight e export continuam fases explícitas.

O crate usa geometria fixed-point, resolvers explícitos de fontes e assets,
recursos limitados, cenas resolvidas imutáveis e falhas tipadas. O formato é
escolhido na chamada de export, nunca no YAML. O crate não depende de
`appcore-ai`; o bridge opcional e a CLI ficam em crates separados.

O shaping usa somente bytes de fontes registrados explicitamente ou métricas de
faces PDF Standard. A lista ordenada de fallback faz parte do fingerprint, e o
embedding SVG/HTML segue as fontes com contornos presentes nos glyph runs.
Patches de runtime são aplicados antes da medição e do layout, portanto a
geometria é sempre recalculada a partir do IR alterado.
O JSON canônico do fingerprint é dimensionado e hasheado em duas passagens por
writer sob o budget agregado `max_output_bytes`; os bytes V1 permanecem
idênticos sem reter um segundo buffer JSON completo.
Para PDF editável, `FontManager::register_pdf_standard` registra explicitamente
uma face latina PDF Standard 14. Larguras e kerning AFM orientam o layout; a
saída referencia uma face Type 1 sem buscar fontes no host nem incorporá-las.
Esse caminho é somente PDF e WinAnsi; caracteres sem representação falham
fechado ou usam fallback configurado explicitamente. SVG, HTML, raster e PDF
flatten precisam de contornos explícitos. Symbol e ZapfDingbats ainda não são
suportadas. Cada run Standard 14 é emitido como uma operação nativa de texto do
PDF; as métricas AFM continuam orientando o layout, enquanto o leitor aplica
os avanços nativos da face na pintura.
```rust
fonts.register_pdf_standard("Helvetica", PdfStandardFont::Helvetica)?;
```
O arquivo de licença e a atribuição AFM ficam em `LICENSE-APAFML` e
`THIRD-PARTY-NOTICES.md`.
`text_options.writing_mode: vertical` molda colunas de cima para baixo que
avançam da direita para a esquerda. Medição e quebra acontecem uma vez no
layout; PDF, SVG, PNG/JPEG e HTML consomem as mesmas colunas e runs moldados.

O texto é limitado por `ResourceLimits::max_text_bytes` (4 MiB por padrão) e
por um teto absoluto de 4 MiB no motor. É possível reduzir o limite configurado,
mas aumentá-lo não eleva o teto do motor. Texto acima do limite é rejeitado,
nunca truncado. Texto expandido fora de tabelas pode continuar entre páginas
em limites de linhas moldadas. Uma linha/célula de tabela é indivisível e
precisa caber no corpo da página; se exceder, o layout falha em vez de cortar
ou descartar conteúdo. Estilos distintos podem ser compostos com elementos ou
linhas de texto separados, mas runs inline mistos num único nó ou célula não
fazem parte do contrato YAML.

Em processos long-lived, use os construtores de `OperationLog` e `SceneCache`
limitados por bytes, `BorrowedDataset` para linhas que já estão em memória e a
API writer. PNG e JPEG renderizam faixas verticais limitadas e as codificam
diretamente nesse writer; o PNG da máscara de colisão usa o mesmo caminho.
O encoder não acumula todas as faixas nem retém o output completo, mas o writer
do caller pode fazê-lo. JPEG libera a faixa anterior antes de renderizar a nova;
falha não deixa superfície stale. Scratch do codec, assets e buffers do caller
são separados.
As fronteiras internas rejeitam dimensões/altura de faixa zero antes de escrever
ou renderizar, e faixas maiores que o plano antes de alocar uma superfície.
CSV, SVG e HTML também transmitem incrementalmente. PDF faz uma passagem limitada de
dimensionamento e então emite objetos independentes e sua tabela de referências
cruzadas rastreada sem reter um buffer final do documento.
JSON, SVG e PDF da máscara de colisão seguem a mesma regra de dimensionamento
antes da escrita e serializam direto no writer do chamador. PDF emite objetos
independentes, um content stream de tamanho exato e seu xref clássico sem reter
o stream da página nem o arquivo completo; o helper JSON que retorna bytes
dimensiona primeiro e aloca somente o resultado exato aceito.

PDF suporta texto editável, flattened e híbrido. O modo híbrido desenha outlines
determinísticos das fonts para a aparência e adiciona uma camada Unicode
invisível e subsetada para busca, seleção e extração, sem reflow no exporter.
O planejamento de fluxo distribuído conta os filhos visíveis sem alocar uma
lista temporária de referências, preservando os mesmos cálculos de tamanho e
espaçamento.
A coleta de nomes de assets no fingerprint ordena referências emprestadas,
evitando clonar strings durante a resolução determinística.

O benchmark runtime do crate expõe workloads separados `compile_canvas_yaml`,
`fingerprint_json_4m`, `collision_mask_json_4m`, `a4_report_end_to_end` e
`a4_report_pdf_hybrid`. `a4_report_export_matrix` executa o mesmo pipeline de
duas páginas com YAML/dados/patch/medição/layout/colisão e então faz preflight e
stream dos três modos PDF, SVG, HTML semântico e fixo, PNG, JPEG e CSV do dataset
para sinks sem retenção. Ele mediu 70,56 ms p50, 71,34 ms p95, MAD de 0,22 ms e
10,64 MiB de RSS pico no Apple M1. `collision_mask_pdf_100k` também grava um PDF
de 1.800.626 bytes a partir de 100.000 retângulos resolvidos; o caso JSON da
máscara grava 4.188.826 bytes em um sink sem retenção.
A resolução de camadas de página agora percorre os elementos ativos de forma
lazy em cada página física, sem lista temporária de referências e preservando a
ordem das roles.

```bash
cargo run -p appcore-filemaker --example basic
cargo run -p appcore-filemaker --example intermediate
```

Cada runner Rust carrega um documento `.yml` separado em `examples/`; o YAML do
template não fica embutido no código Rust. O runner básico grava um SVG completo
de uma página; o intermediário grava PDF de duas páginas, HTML fixo, previews
SVG por página e um relatório de preflight estrito em
`target/filemaker-examples/`. Os dados tipados também ficam em arquivos JSON
separados, e a fonte Noto Sans exata, sob OFL, acompanha o exemplo para um
resultado portátil e determinístico. Veja a
[arquitetura](wiki/architecture.pt.md), o [exemplo básico](wiki/examples/basic.pt.md)
e o [exemplo intermediário](wiki/examples/intermediate.pt.md).

Licença: MIT.

## Documentação estável

ID estável: **ACR-023**. Consulte o
[guia complementar de arquitetura e integração](https://wiki.appcore.dnettoraw.com/pt/crates/id/acr-023). Esse ID permanente
continua válido se a página da wiki mudar.
Para atualizar código Rust entre betas, consulte o
[guia de migração do crate](wiki/migration.pt.md).

Use `audit_layout` com `LayoutSafetyOptions` depois de resolver uma cena. O
`LayoutSafetyReport` limitado resume overflow, colisões e problemas de texto,
pode impor uma política estrita sem avisos e gera JSON determinístico para
fixtures golden. Ele reutiliza as mesmas verificações de medição, quebra,
paginação e colisão usadas no export, sem criar um segundo modelo geométrico.
