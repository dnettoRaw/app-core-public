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

Registre bytes exatos de fonts e uma lista ordenada de fallback antes da
medição; a ordem entra no fingerprint e exporters incorporam as famílias
realmente escolhidas nos glyph runs resolvidos. Aplique patches de runtime no
binding, antes do layout, para que medição, colisão, paginação e export usem
geometria recalculada.
O JSON do fingerprint usa uma passagem de dimensionamento seguida de hashing
direto sob o budget agregado `max_output_bytes`. Ele preserva o framing V1
exato sem reter os bytes JSON canônicos.

Para japonês vertical ou layouts semelhantes, use
`text_options.writing_mode: vertical`. O engine quebra pelo limite de altura,
molda cada coluna de cima para baixo e avança as colunas da direita para a
esquerda. Mantenha `horizontal` (o padrão) para texto horizontal e BiDi.

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
