# appcore-filemaker

Diagnostic messages are shortened only at UTF-8 boundaries: errors retain at
most 1,024 bytes; source paths, validation issue messages and export loss
messages retain at most 512. Oversized backing buffers are replaced with the
bounded prefix. This bounds retained diagnostic text, not the caller's prior
allocation, allocator overhead or the number of accumulated reports.

For PNG/JPEG, `export_raster_controlled` accepts `RasterOptions::new(bytes,
rows)` without changing `ExportRequest`. Defaults remain 4 MiB and 256 rows;
accepted limits are 1 byte–64 MiB and 1–4096 rows, with a separate 4 MiB
scanline ceiling. A scanline that cannot fit is rejected before encoding.
For example, `RasterOptions::new(1024 * 1024, 64)?` caps the working surface
at 1 MiB/64 rows. Smaller strips can require more repeated rendering, especially
for JPEG block traversal. Non-raster formats are rejected; layout, loss reporting
and paint overrides remain shared. Existing exports, CLI/AI and debug masks use
the defaults. This bounds the surface, not codec/assets/output or process RSS.

`reflow_dense_64` resolves 64 initially overlapping rectangles with a 64-attempt
limit and checks every final position. `reflow_limit_63` uses the same fixture
but requires the explicit iteration-limit error at 63 attempts. Compilation and
binding are outside timing; engine setup, layout/reflow, result checks and
teardown are inside. These cases do not isolate text measurement or collision
lookup costs and do not exercise a geometric cycle.

The `diagnostic_geometry_256` benchmark derives a Combined debug mask and
queries free regions on a resolved 16-by-16 rectangle grid. It checks duplicate
suppression, absence of collisions/overflow and identical free-region results.
Compile/bind/layout prepare the fixture outside timing; validation, derivation,
query, result checks and output teardown are timed. It does not measure dense
reflow, text measurement or exporter encoding.

Successful diagnostic rectangle subtraction now keeps at most four temporary
pieces inline instead of allocating a `Vec` per comparison. Free-region queries
and debug-mask derivation share this helper and preserve strip order and budgets.
The retained region lists still allocate; this is not a process-memory limit
or a measured claim of lower RSS or higher throughput.

Debug bounds selection and per-element mask deduplication also use fixed
four-slot storage. Combined masks still remove duplicate bounds per element;
overlays retain each selected bounds class in its original order.

`SceneInspector::query_free_regions_controlled` accepts `OperationControl`.
It checks cancellation around scene validation and final filtering/sorting,
and reports completed rectangle subtractions in the Preflight phase. These
validation/sort passes are not internally interruptible. The query subtracts
resolved collision bounds and exclusions, respects diagnostic budgets, and
never reads a debug mask or changes the scene. Cancellation discards partial
results; synchronous observers must return promptly. Existing query methods
retain their behavior without allocating a control token.

`export_dataset_csv_controlled` accepts `OperationControl`, checks cancellation
before output and at row boundaries, and reports completed rows in the Export
phase. A cancelled export returns `Cancelled`; the writer can retain a partial
CSV prefix which the caller must discard or roll back. Dataset/writer callbacks
are cooperative, not preemptible. The AI bridge uses the same control for CSV.

Cache size accounting stops as soon as serialized bytes exceed its budget;
it does not encode or traverse the remaining scene merely to reject it.

On a cache miss, fully occupied consumer-held entry/byte capacity is rejected
before invoking the resolver, without evicting cached entries. Hits remain
available. This precheck is conservative under concurrent lease drops and is
not a scratch-memory reservation; final insertion still validates admission.

SceneCache admission counts both cached scenes and evicted scenes still held
by consumers. `used_bytes()` reports cached serialized bytes; `retired_bytes()`
reports observed evicted-but-live bytes. Entry and byte limits can reject an
insertion while an earlier Arc remains alive. FIFO eviction may occur before
that rejection; release old handles before retrying. Weak tracking does not
keep scenes alive and its entry count is bounded by cache capacity. These are
serialized-size budgets, not heap/RSS accounting: compilation scratch, consumer
copies and Arc::make_mut allocations remain outside the cache's control.

**Pre-1.0 beta.** Validate outputs, limits and failure handling for your
workload; package tests are not production certification.

[Português](README.pt.md) | [Français](README.fr.md)

Migration notes: [English](wiki/migration.en.md) | [Português](wiki/migration.pt.md) | [Français](wiki/migration.fr.md)

## Generic document layout contract

Text alignment is resolved after the final font measurement. Use
`text_options.align_x: start|center|end` for a text element and
`table.columns[].align_x` for a table column. The resulting line offset is
shared by PDF, SVG, raster, and HTML export; it is not implemented with
application-specific spacer text or font-size tricks. `deny_unknown_fields`
continues to reject unsupported YAML properties.
Set `text_options.padding_inline: 4pt` for symmetric inline padding. Wrapping
uses the reduced content width and retains the inset on every line, including
table cells using the table element's shared text options.
For same-style one-line compositions, `text_segments` accepts ordered literal
or string-bound pieces with optional `gap_after` lengths. Set
`text_options: { overflow: error, max_lines: 1 }`; every segment and gap is
measured as one aligned line and preserved by PDF, SVG, raster, and HTML.
Segments cannot wrap independently; compose multiline or mixed-style content
from separately styled flow elements.
Text elements also accept block padding, for example
`text_options.padding: { top: 2pt, right: 4pt, bottom: 2pt, left: 4pt }`.
These per-side insets reduce measured content bounds and are retained by PDF,
SVG, raster, and HTML export; expanded text pagination reserves the same
vertical insets on each fragment. Nonnegative absolute, logical, and sub-50%
lengths are supported (`auto` is rejected); horizontal percentages use the
element width and vertical percentages use its height.
Leading paragraph indentation is preserved on continuation lines after wrapping.
Each `table.columns[]` entry may also declare
`padding: { top, right, bottom, left }` using nonnegative absolute or logical
lengths. The insets reduce cell measurement width and height and are shared by
all exporters; auto-width columns include horizontal insets in their measured
size. Conditional table rules may declare the same `padding` object to add
per-side insets to cells in matching rows. The last matching rule that declares
padding wins; its insets are added to column insets and participate in
measurement, pagination, and export. Use `padding_first_page` and
`padding_continuation` on a conditional rule to override those insets by page
role; an omitted page override falls back to `padding`.
`style.line_height` overrides the shared text line-height for matching styles,
including conditional table rows. It is a ratio in millionths from 500000 to
4000000; for example, `line_height: 1250000` means 1.25.

For Rust struct literals, initialize `align_x` and `padding_inline` on
`TextOptions`; `padding` on `TextLayout` defaults to `Insets::default()` and
`TextSourceOptions` uses `TextBlockPadding::default()`. `TableColumn` initializes
`align_x` and `padding` (`Alignment::Start`
and `CellPadding::default()` preserve prior behavior). Deserialized
table-column YAML defaults to `start` and zero insets. `TextLine` literals also initialize `source_text`; older serialized
scenes deserialize this field as empty, but text pagination requires freshly
shaped layouts carrying the source lines.

Named anchors to a paginated table resolve against its final physical
fragment and its page. A following flow element can continue after the table
without scanning resolved scenes.
For an exact row, set `table.row_anchor_field` to a metadata field containing
unique bounded strings, then anchor with `table-id::name.top|bottom`. Missing
or null row metadata publishes no anchor; duplicate names are rejected.
Conditional table rules may set `reserve_after: 18pt` for matching anchored rows.
This positive absolute length reserves pagination capacity for following
content without changing rendered row geometry; the table must declare
`row_anchor_field`.
The executable fixture `examples/row-anchor-reserve.yml` with
`examples/row-anchor-reserve-data.json` demonstrates an anchored row moving to
a continuation page with its following element.

In a vertical flow, set `keep_with_next: true` on every element through the
penultimate member of a contiguous block. If the measured block fits one page
but not the remaining space, the engine starts it on the next page. A block
taller than a page flows element by element; one element taller than the page
fails to prevent blank-page loops. Horizontal text with `overflow: expand` in a
vertical flow splits between complete shaped lines when it exceeds the page
content area; an individual line taller than that area still fails. Other
oversized elements are not split. Rich-text runs and barcode encoding remain
unsupported. `group_by` marks table group starts but makes no
same-page promise. Use `keep_together_by: layout_group` to keep adjacent rows
with the same non-null key on one page when their measured height fits. A larger
group splits at row boundaries; each individual row must still fit a page.
Use `table.page_bodies.first` and `table.page_bodies.continuation` with
`offset_y` and `height` to reserve different first-page and continuation body
rectangles without spacer rows or oversized spacer fonts.
Conditional table styles also support `min_height` per matching row; pagination
uses the larger of measured content and the matching minimum. Set
`min_height_first_page` / `min_height_continuation` when spacing differs by page.

`style.underline` paints one underline per shaped horizontal text line, including
conditional table-cell styles. It is paint-only and is not rendered for vertical
text. Set `style.stroke`, `style.stroke_width`, and optionally
`style.stroke_sides: { top: true, right: false, bottom: true, left: false }`
to select table-cell border edges; omitted sides default to enabled.
Conditional table rules also accept `text_offset_y` for a paint-only vertical
translation. It leaves measured content, row heights, and pagination unchanged
and is clipped to the original cell content bounds in every visual exporter.
Use `text_offset_y_first_page` or `text_offset_y_continuation` to override it
for the corresponding physical page role.
Remaining layout limits: `keep_with_next` applies to sibling elements in
vertical flows. Page templates can provide role-specific headers and footers;
tables can also declare their own first and continuation body rectangles. YAML
has no rich-text runs or arbitrary interleaved aggregate layout. Multiple exact total fields are
supported by tables; custom grouped value presentation remains template
composition. `TextEngine` rejects
individual strings above 4 MiB, and the compiler's default text budget is also
4 MiB. `losses=0` reports exporter feature support; it does not establish
visual equivalence with another renderer. Barcode nodes are reserved and
reported unsupported by the PDF/SVG/raster/HTML exporters, including EAN13.

Deterministic AppCore compiler for declarative documents, semantic vector
canvases, and bounded datasets. Versioned `filemaker: "1.0"` YAML is only a
frontend: compilation, data binding, layout, collision, inspection, preflight,
and export remain explicit phases.

The crate uses fixed-point geometry, explicit font and asset resolvers, bounded
resources, immutable resolved scenes, and typed failures. Export format is
selected at the export call, never in YAML. The crate does not depend on
`appcore-ai`; the optional bridge and CLI live in separate crates.

Text shaping uses only explicitly registered font bytes or PDF Standard face
metrics. The ordered fallback list is part of the document fingerprint, and
SVG/HTML embedding follows outline fonts in the resolved glyph runs. Runtime
patches are applied before measurement and layout, so geometry is always
recomputed from the patched IR.
For editable PDF, `FontManager::register_pdf_standard` explicitly registers a
Latin PDF Standard 14 face. AFM widths and kerning drive measured layout, while
the output references a Type 1 face without host font discovery or embedding.
This path is PDF-only and WinAnsi-only; unsupported characters fail closed or
use an explicitly configured fallback. SVG, HTML, raster, and flattened PDF
need explicit outlines. Symbol and ZapfDingbats are not supported yet.
Each Standard 14 run is emitted as one native PDF text-show operation; AFM
metrics still drive layout, while the PDF viewer applies native string advances.
```rust
fonts.register_pdf_standard("Helvetica", PdfStandardFont::Helvetica)?;
```
The AFM data license and attribution are retained in
[`LICENSE-APAFML`](LICENSE-APAFML) and [`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md).
Canonical fingerprint JSON is sized and hashed in two writer passes under the
aggregate `max_output_bytes` budget; the V1 bytes remain identical without
retaining a second full JSON buffer.
`text_options.writing_mode: vertical` shapes top-to-bottom columns flowing from
right to left. Measurement and wrapping happen once in layout; PDF, SVG,
PNG/JPEG, and HTML consume the same resolved columns and shaped runs.

Text input is bounded by `ResourceLimits::max_text_bytes` (4 MiB by default)
and by a hard 4 MiB text-engine ceiling. Lower configured limits are supported;
raising the configured limit does not raise the engine ceiling. Oversized text
is rejected, not truncated. Expanded standalone text can continue across pages
at shaped-line boundaries. A table row/cell is currently indivisible and must
fit in a page body; an oversized row fails layout rather than being clipped or
dropped. Distinct styles can be composed from separate text elements/lines,
but inline mixed-style runs in one source node or cell are not in the YAML
contract.

For long-lived processes, use byte-bounded `OperationLog` and `SceneCache`
constructors, `BorrowedDataset` for rows already in memory, and the writer API.
PNG and JPEG render bounded vertical strips and encode them directly to that
writer; collision-mask PNG uses the same path. The encoder does not collect all
strips or retain the complete encoded output, though the caller's writer may.
JPEG releases its cached strip before rendering the replacement; render failure
leaves no stale surface. Codec scratch, assets and caller buffers remain separate.
Internal raster boundaries reject zero dimensions or strip height before
output/rendering, and reject requests exceeding the planned strip height
before allocating a surface.
CSV, SVG, and HTML also stream incrementally. PDF performs a bounded sizing
pass, then emits independent objects and its tracked cross-reference table
without retaining a final document buffer.
Collision-mask JSON, SVG, and PDF use the same pre-write sizing rule and
serialize directly to the caller's writer. PDF emits independent objects, an
exact-length content stream, and its classic xref without retaining either the
page stream or complete file; the byte-returning JSON helper sizes first and
allocates only its exact accepted result.

PDF supports editable, flattened, and hybrid text. Hybrid draws deterministic
font outlines for appearance, then adds an invisible subsetted Unicode text
layer for search, selection, and extraction without exporter-side reflow.
Distributed flow planning counts visible children without allocating a temporary
reference list, while preserving the same size and spacing calculations.
Fingerprint asset-name collection borrows names while sorting, avoiding cloned
strings during deterministic asset resolution.

The crate runtime benchmark exposes separate `compile_canvas_yaml`,
`fingerprint_json_4m`, `collision_mask_json_4m`, `a4_report_end_to_end`, and
`a4_report_pdf_hybrid` workloads. `a4_report_export_matrix` executes the same
two-page YAML/data/patch/measurement/layout/collision pipeline, then preflights
and streams all three PDF modes, SVG, semantic and fixed HTML, PNG, JPEG, and
dataset CSV to non-retaining sinks. It measured 70.56 ms p50, 71.34 ms p95,
0.22 ms MAD, and 10.64 MiB peak RSS on Apple M1. `collision_mask_pdf_100k`
additionally writes a 1,800,626-byte PDF from 100,000 resolved rectangles. The
JSON mask case writes 4,188,826 bytes to a non-retaining sink.
Page-layer resolution now iterates active elements lazily per physical page,
avoiding a temporary reference list while preserving page-role ordering.

```bash
cargo run -p appcore-filemaker --example basic
cargo run -p appcore-filemaker --example intermediate
```

Each Rust runner loads a separate `.yml` document from `examples/`; template
YAML is not embedded in Rust source. The basic runner writes a complete
one-page SVG; the intermediate runner writes a two-page PDF, fixed HTML,
page-specific SVG previews, and a strict preflight report under
`target/filemaker-examples/`. Typed example data is also kept in separate JSON
files, and the exact OFL-licensed Noto Sans font is bundled for portable,
deterministic output. See the [English architecture](wiki/architecture.en.md),
[basic example](wiki/examples/basic.en.md), and
[intermediate example](wiki/examples/intermediate.en.md).

License: MIT.

## Stable documentation

Stable ID: **ACR-023**. See the
[supplemental architecture and integration guide](https://wiki.appcore.dnettoraw.com/crates/id/acr-023). This permanent ID
remains valid if the wiki page moves.
For Rust source updates between betas, see the crate-owned
[migration guide](wiki/migration.en.md).

Use `audit_layout` with `LayoutSafetyOptions` after resolving a scene. The
bounded `LayoutSafetyReport` summarizes overflow, collision and text findings,
can enforce a strict no-warning policy, and serializes deterministic JSON for
golden evidence. It reuses the same measurement, wrapping, pagination and
collision checks used by export; it does not invent a second geometry model.
