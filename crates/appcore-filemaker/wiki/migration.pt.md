# Migração de layouts declarativos

Este guia descreve as mudanças aditivas de YAML e API Rust do FileMaker beta
atual. Fixe a versão do crate e os arquivos de fonte usados em cada PDF de
referência; `losses=0` do exporter não comprova paridade visual.

## Alinhamento e padding de texto

- Substitua alinhamento de colunas numéricas com espaços/NBSP por
  `text_options.align_x` ou `table.columns[].align_x` (`start`, `center`,
  `end`). O alinhamento usa a largura moldada final após wrap e shrink.
- Use `text_options.padding_inline` para recuo simétrico preservado no wrap.
  Defina `table.columns[].padding: { top, right, bottom, left }` para insets
  por lado; valores devem ser comprimentos absolutos ou lógicos não negativos.
  Use `text_options.padding: { top, right, bottom, left }` para insets de bloco
  em texto; eles reduzem os limites medidos e são compartilhados por exporters
  e paginação de texto expandido. São aceitos valores absolutos/lógicos não
  negativos e percentuais abaixo de 50%; `auto` é rejeitado.
- Literais Rust inicializam os campos novos listados na seção de migração beta3
  abaixo. Use `Default` quando implementado para preservar o comportamento
  padrão. TextLines serializados antigos leem `source_text` como vazio; remolde
  o texto antes de paginá-lo por linhas.

## Mudanças em literais Rust desde beta3

O contrato YAML `filemaker: "1.0"` recebe campos opcionais compatíveis, mas a API
Rust beta não preserva compatibilidade de código-fonte para literais públicos
exaustivos. Os campos beta3–beta5 detectados por `cargo-semver-checks` são:

- `TextOptions`: `align_x`, `padding_inline`; `TextSourceOptions`:
  `align_x`, `padding_inline`, `padding`; `TextIr`: os mesmos três; `TextLayout`:
  `paint_offset_y`, `align_x`, `padding_inline`, `padding`; `TextLine`:
  `source_text`.
- `ElementSource` e `ElementIr`: `keep_with_next`, `text_segments`.
- `StyleSource`, `Style` e `ComputedStyle`: `stroke_sides`, `line_height`,
  `underline`.
- `TableColumn` e `ResolvedTableColumn`: `align_x`, `padding`;
  `ResolvedTableCell`: `padding`.
- `TableSource`: `keep_together_by`, `row_anchor_field`, `page_bodies`;
  `TableSpec`: `keep_together_by`, `row_anchor_field`; `TableIr`: `page_bodies`;
  `TablePage`: `row_padding`.
- `TableStyleRuleSource` e `TableStyleRule`: `padding`, `padding_first_page`,
  `padding_continuation`, `text_offset_y`, `text_offset_y_first_page`,
  `text_offset_y_continuation`, `min_height`,
  `min_height_first_page`, `min_height_continuation`, `reserve_after`.

Consumidores Rust externos que constroem esses valores públicos devem adicionar
os campos listados (ou usar `..Default::default()` somente nos tipos que
implementam `Default`). Esta é uma migração beta, não uma afirmação de que os
literais Rust de beta3 continuam compilando. O verificador classifica beta3–beta5
como uma etapa major de pré-lançamento; forçar compatibilidade patch falha nos
campos adicionados.

## Paginação

- Mantenha elementos irmãos relacionados com `keep_with_next` até o penúltimo
  elemento visível de um fluxo vertical.
- Para linhas de tabela, forneça uma chave de metadados não nula em cada linha
  e configure `table.keep_together_by: layout_group`. Chaves iguais e contíguas
  ficam na mesma página se a altura medida couber. `group_by` apenas marca
  inícios; não garante união. Grupos maiores são divididos entre linhas
  completas; cada linha precisa caber. Represente um bloco lógico maior que a
  página em várias linhas componentes com a mesma chave; valores incluídos nos
  totais devem aparecer em apenas uma delas para evitar agregação duplicada.
- Texto horizontal com `overflow: expand` em fluxo vertical é dividido nos
  limites de linhas moldadas. Uma linha maior que a área da página falha.
- Âncoras nomeadas de tabelas paginadas resolvem para o último fragmento
  físico. Para apontar a uma linha, declare `row_anchor_field` e forneça um
  nome único e limitado nesse campo de metadados; por exemplo,
  `anchors: { top: 'linhas::totais.bottom+4pt' }` aponta à linha `totais` e a
  acompanha na página física correspondente. Campo ausente ou `null` não cria
  âncora.
- Para manter um elemento seguinte na página da linha ancorada, adicione
  `reserve_after` absoluto positivo à regra condicional correspondente. Veja o
  fixture executável `examples/row-anchor-reserve.yml` e os dados correspondentes.
- Uma regra correspondente de `conditional_styles` pode definir
  `min_height: 18pt`. A linha usa o maior valor entre a altura medida do
  conteúdo e os mínimos aplicáveis, permitindo espaçamento por tipo sem fonte
  espaçadora exagerada. Use `min_height_first_page` e
  `min_height_continuation` quando as áreas por página exigirem espaçamentos
  diferentes; a paginação mede conforme a página de destino.
- Defina `style.line_height: 1250000` em um estilo condicional de linha para
  substituir o line-height compartilhado (razão em milionésimos, de 500000 a 4000000).

Use `padding` por lado em regras `conditional_styles` correspondentes para
recuo visual. Vence o último padding correspondente, somado ao padding da
coluna; não prefixe os dados das linhas com espaços para simular layout.
Use `padding_first_page` ou `padding_continuation` quando a mesma linha precisar
de insets diferentes conforme a página de destino. Um valor específico ausente
usa o `padding` daquela regra como fallback.

## Limites restantes

Rich text e códigos de barras (incluindo EAN13) não têm suporte. Texto
independente aceita padding por lado no eixo de bloco via
`text_options.padding`, medido e exportado de forma consistente. Bordas de
células podem selecionar lados com
`style.stroke_sides`; `style.underline` desenha cada linha horizontal medida.
Use `table.page_bodies.first` e `.continuation` com `offset_y` e `height` para
áreas distintas da tabela na primeira página e continuações. Quando o modelo em linhas servir,
represente estilos em linhas separadas com `conditional_styles` e
`keep_together_by`. O Runtime organiza e exporta dados tipados; cálculos e
rótulos do domínio pertencem à aplicação. Exemplos de documentos específicos
da aplicação ficam nos respectivos repositórios, não neste crate genérico.

Após migrar, rasterize e inspecione todas as páginas. Inclua fixtures de uma,
duas, três e quatro ou mais páginas, anotações perto das quebras, textos longos,
larguras numéricas variadas, vários grupos de linhas e linhas finais próximas
ao rodapé.
