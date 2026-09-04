# Exemplo intermediário

[English](intermediate.en.md) | [Français](intermediate.fr.md) | [Básico](basic.pt.md)

Execute `cargo run -p appcore-filemaker --example intermediate`. Ele monta um
relatório confidencial plausível de desempenho de serviço com exatamente duas
páginas A4, numeração `Page {page} of {pages}`, watermark rotacionado repetido,
cards de KPI, estilo condicional, dois formatos de gráfico vetorial e tabela de
primeira classe agrupada que continua na segunda página. Também exercita tokens
de tema herdados, dependência computada, anchors por guide, patch atômico,
`OperationLog` limitado, fingerprint determinístico, cache de cena, inspeção e
preflight PDF estrito. O documento completo é o arquivo separado
[`examples/intermediate.yml`](../../examples/intermediate.yml), carregado por
[`examples/intermediate.rs`](../../examples/intermediate.rs), com dados tipados
em [`examples/intermediate-data.json`](../../examples/intermediate-data.json).
O runner registra explicitamente a Noto Sans sob OFL incluída e grava PDF
editável, HTML fixo, os dois SVGs de página e o relatório JSON de preflight em
`target/filemaker-examples/`. Os gráficos usam primitivas vetoriais semânticas,
pois o node `chart` de primeira classe está preparado, mas ainda não é uma
capacidade 1.0 implementada.

Para imagens, passe o mesmo resolver explícito para
`LayoutEngine::with_assets` e `ExportContext`. Fit, crop, foco, aspecto,
orientação EXIF e DPI efetivo são resolvidos antes do export.

O espaço de cor é explícito e tokens podem resolver cores funcionais:

```yaml
themes:
  print:
    tokens: { ink: "cmyk(1000000, 250000, 0, 100000)" }
theme: print
style:
  fill: { space: gray, value: 245 }
  stroke: $ink
  opacity: 900000
style_rules:
  - when: data.highlight == true
    style: { fill: "rgba(255, 220, 0, 192)" }
```

As regras executam em ordem durante o binding. Um patch transacional
`SetStyle` posterior pode mudar qualquer campo de style medido antes do layout.
`ExportStyleOverride` é a layer global final e expõe somente fill, stroke,
opacity e cor de texto; ele não pode provocar reflow dentro do exporter.

Use `FontManager::register_from(&resolver, "fonts/Body.ttf", max_bytes)` com
um `MemoryResolver` explícito ou `FileResolver` de raiz canônica; nenhuma font
do host é descoberta automaticamente.

Um transform permanece independente do formato e fixed-point até o layout:

```yaml
transform:
  translate_x: 4mm
  rotate: 45
  scale_x: 1250000
  mirror: vertical
  origin_x: 50%
  origin_y: 50%
```

Coordenadas do Canvas podem combinar unidades físicas, de tela, relativas,
lógicas e normalizadas enquanto o path permanece semântico:

```yaml
- id: curve
  type: path
  x: 10px
  y: 2mm
  width: 50%
  height: 12lu
  path:
    - { command: move, x: 0norm, y: 1norm }
    - { command: curve, x1: 0.25norm, y1: 0norm, x2: 0.75norm, y2: 0norm, x: 1norm, y: 1norm }
    - { command: close }
```

Overflow de texto também é independente do formato e medido antes da colisão:

```yaml
text_options:
  overflow: ellipsis
  max_lines: 2
  min_font_size: 8pt
  line_height: 1250000
```

Constraints e anchors por guide também permanecem declarativos:

```yaml
guides: { column: 25% }
elements:
  - id: card
    type: rect
    constraints: { preferred_width: 40mm, min_width: 30mm, max_width: 50mm, aspect_ratio: 1600000 }
    align_y: center
    anchors: { left: "guide:column+2mm" }
```

Reserve geometria não pintada antes do layout com uma exclusão nomeada:

```yaml
exclusions:
  header-clearance:
    x: 0pt
    y: 0pt
    width: 100%
    height: 18mm
    collides_with: [content]
```

Elementos no grupo de colisão `content` fazem reflow contra esse retângulo em
todas as páginas; exporters não pintam a exclusão.

Para filhos de flow com tamanho fixo, adicione `distribute: space_between` (ou
`center`, `end`, `space_around`, `space_evenly`) ao grupo.

Páginas de documento podem compor um master com uma layer específica do papel.
Cada layer tem bandas background/header/footer sem colisão, e a numeração só é
resolvida depois da paginação:

```yaml
page:
  preset: A4
  master:
    footer:
      - { id: number, type: text, text: "Página {page}/{pages}", x: 15mm, y: 280mm, width: 60mm, height: 5mm, style: { font: Body, font_size: 8pt } }
  first:
    header:
      - { id: first-title, type: rect, x: 15mm, y: 8mm, width: 180mm, height: 8mm }
  continuation:
    header:
      - { id: continued, type: rect, x: 15mm, y: 8mm, width: 180mm, height: 4mm }
  last:
    footer:
      - { id: final-rule, type: line, x: 15mm, y: 275mm, width: 180mm, height: 1pt }
```

Registre a fonte exata `Body` antes do layout, como para qualquer texto.

Uma tabela de primeira classe mantém as linhas tipadas para medição e
paginação posteriores:

```yaml
- id: results
  type: table
  binding: data.rows
  table:
    columns:
      - { field: name, header: Name, width: { mode: flex, value: 1 } }
      - { field: amount, header: Amount, width: { mode: auto } }
    repeat_header: true
    total_fields: [amount]
    max_rows: 1000
    max_cell_bytes: 4096
    row_height: auto
```

Limites locais podem ser mais restritos que `ResourceLimits`, nunca maiores.

Depois da resolução, debug e inspeção continuam somente leitura:

```rust
let explanation = SceneInspector::new(&scene).explain_layout(&ElementId::new("curve")?)?;
let mask = CollisionMask::derive(&scene, 0, MaskView::Combined)?;
let json = mask.to_json()?;
```

A explicação preserva geometria de origem, anchors, medição, colisão,
página/reflow e provenance. O JSON da mask distingue geometria ocupada, livre,
de colisão e overflow sem consultar pixels renderizados.

Para output limitado em memória, use o mesmo request e context da API writer:

```rust
let (bytes, outcome) = export_bytes(&scene, &request, &context)?;
assert_eq!(bytes.len(), outcome.bytes_written);
```

Use `export` com um writer do chamador para output direto. CSV de Dataset
continua streaming por linha através de `export_dataset_csv`; use
`BorrowedDataset::new(&rows)` quando as linhas já pertencem ao documento.

Para compile-once/render-many, calcule
`DocumentFingerprint::compute_with_patches` com template, dados, patches,
assets e fonts exatos e chame `LayoutEngine::resolve_cached`. Um fingerprint
repetido retorna o mesmo `Arc<ResolvedScene>` imutável sem refazer layout;
eviction FIFO limitada e verificação da versão do engine mantêm o cache explícito.
Configure os dois budgets com `SceneCache::with_byte_capacity(entries, bytes)`.
Configure undo/redo com `OperationLog::new_bounded(entries, bytes)` para que
documentos grandes não esgotem o processo apenas porque há poucas entradas.
