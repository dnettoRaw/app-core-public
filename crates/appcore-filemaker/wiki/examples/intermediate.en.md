# Intermediate example

[Português](intermediate.pt.md) | [Français](intermediate.fr.md) | [Basic](basic.en.md)

Run `cargo run -p appcore-filemaker --example intermediate`. It builds a
plausible confidential service-performance review with exactly two A4 pages,
`Page {page} of {pages}` numbering, a repeated rotated watermark, KPI cards,
conditional styling, two vector chart forms, and a grouped first-class table
that continues onto the second page. It also exercises inherited theme tokens,
a computed-data dependency, guide anchors, an atomic patch, a bounded
`OperationLog`, deterministic fingerprinting, scene caching, inspection, and
strict PDF preflight. The complete document is the separate
[`examples/intermediate.yml`](../../examples/intermediate.yml) file loaded by
[`examples/intermediate.rs`](../../examples/intermediate.rs), with typed input
in [`examples/intermediate-data.json`](../../examples/intermediate-data.json).
The runner explicitly registers the bundled OFL Noto Sans font and writes an
editable PDF, fixed HTML, both page SVGs, and the JSON preflight report under
`target/filemaker-examples/`. Charts use semantic vector primitives because the
prepared first-class `chart` node is not yet an implemented 1.0 capability.

For images, give the same explicit resolver to `LayoutEngine::with_assets` and
`ExportContext`. Fit, crop, focal point, aspect, EXIF orientation, and effective
DPI are then resolved before export.

Color space is explicit and tokens may resolve to functional colors:

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

Rules run in order during binding. A later transactional `SetStyle` patch can
change any measured style field before layout. `ExportStyleOverride` is the
final global layer and intentionally exposes only fill, stroke, opacity, and
text color; it cannot trigger exporter-side reflow.

Use `FontManager::register_from(&resolver, "fonts/Body.ttf", max_bytes)` with
an explicit `MemoryResolver` or canonical-root `FileResolver`; no host font is
discovered automatically.

A transform remains format-neutral and fixed-point until layout:

```yaml
transform:
  translate_x: 4mm
  rotate: 45
  scale_x: 1250000
  mirror: vertical
  origin_x: 50%
  origin_y: 50%
```

Canvas coordinates can mix physical, display, relative, logical, and
normalized spellings while the path remains semantic:

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

Text overflow is also format-neutral and measured before collision:

```yaml
text_options:
  overflow: ellipsis
  max_lines: 2
  min_font_size: 8pt
  line_height: 1250000
```

Constraints and guide anchors remain declarative too:

```yaml
guides: { column: 25% }
elements:
  - id: card
    type: rect
    constraints: { preferred_width: 40mm, min_width: 30mm, max_width: 50mm, aspect_ratio: 1600000 }
    align_y: center
    anchors: { left: "guide:column+2mm" }
```

Reserve non-painted geometry before layout with a named page exclusion:

```yaml
exclusions:
  header-clearance:
    x: 0pt
    y: 0pt
    width: 100%
    height: 18mm
    collides_with: [content]
```

Elements in collision group `content` reflow against that rectangle on every
page; exporters do not paint the exclusion.

For fixed-size flow children, add `distribute: space_between` (or `center`,
`end`, `space_around`, `space_evenly`) to the group.

Document pages can compose a master with one role-specific layer. Each layer
has collision-free background/header/footer bands, and numbering is resolved
only after pagination:

```yaml
page:
  preset: A4
  master:
    footer:
      - { id: number, type: text, text: "Page {page}/{pages}", x: 15mm, y: 280mm, width: 60mm, height: 5mm, style: { font: Body, font_size: 8pt } }
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

Register the exact `Body` font before layout, as for every text element.

A first-class table keeps bound rows typed for later measurement and
pagination:

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

Local limits may be stricter than `ResourceLimits`, never larger.

After resolution, debug and inspection remain read-only:

```rust
let explanation = SceneInspector::new(&scene).explain_layout(&ElementId::new("curve")?)?;
let mask = CollisionMask::derive(&scene, 0, MaskView::Combined)?;
let json = mask.to_json()?;
```

The explanation retains source geometry, anchors, measurement, collision,
page/reflow, and provenance. The mask JSON distinguishes occupied, free,
collision, and overflow geometry without querying rendered pixels.

For bounded in-memory output, use the same request and context as the writer
API:

```rust
let (bytes, outcome) = export_bytes(&scene, &request, &context)?;
assert_eq!(bytes.len(), outcome.bytes_written);
```

Use `export` with a caller-owned writer for direct output. Dataset CSV remains
row-streaming through `export_dataset_csv`; use `BorrowedDataset::new(&rows)`
when rows already belong to the document.

For compile-once/render-many, compute `DocumentFingerprint::compute_with_patches`
from the exact template, data, patches, assets, and fonts, then call
`LayoutEngine::resolve_cached`. A repeated fingerprint returns the same
immutable `Arc<ResolvedScene>` without rerunning layout; bounded FIFO eviction
and engine-version checks keep cache behavior explicit. Configure both budgets
with `SceneCache::with_byte_capacity(entries, bytes)`. Configure undo/redo with
`OperationLog::new_bounded(entries, bytes)` so large documents cannot exhaust
the process merely because the entry count is small.
