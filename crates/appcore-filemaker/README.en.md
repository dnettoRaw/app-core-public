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

**PUBLIC BETA — `0.1.0-beta.2`.** APIs and behavior may change before stable
release. Validate outputs, limits and failure handling for your workload;
implementation and local tests are not production certification.

[Português](README.pt.md) | [Français](README.fr.md)

Deterministic AppCore compiler for declarative documents, semantic vector
canvases, and bounded datasets. Versioned `filemaker: "1.0"` YAML is only a
frontend: compilation, data binding, layout, collision, inspection, preflight,
and export remain explicit phases.

The crate uses fixed-point geometry, explicit font and asset resolvers, bounded
resources, immutable resolved scenes, and typed failures. Export format is
selected at the export call, never in YAML. The crate does not depend on
`appcore-ai`; the optional bridge and CLI live in separate crates.

Text shaping uses only registered font bytes. The ordered fallback list is
part of the document fingerprint, and SVG/HTML embedding follows the fonts in
the resolved glyph runs. Runtime patches are applied before measurement and
layout, so geometry is always recomputed from the patched IR.
Canonical fingerprint JSON is sized and hashed in two writer passes under the
aggregate `max_output_bytes` budget; the V1 bytes remain identical without
retaining a second full JSON buffer.
`text_options.writing_mode: vertical` shapes top-to-bottom columns flowing from
right to left. Measurement and wrapping happen once in layout; PDF, SVG,
PNG/JPEG, and HTML consume the same resolved columns and shaped runs.

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

Use `audit_layout` with `LayoutSafetyOptions` after resolving a scene. The
bounded `LayoutSafetyReport` summarizes overflow, collision and text findings,
can enforce a strict no-warning policy, and serializes deterministic JSON for
golden evidence. It reuses the same measurement, wrapping, pagination and
collision checks used by export; it does not invent a second geometry model.
