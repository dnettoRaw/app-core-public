# appcore-filemaker

> **BETA PÚBLICA** — version `0.1.0-beta.1` publiée sur crates.io.

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
writer; collision-mask PNG uses the same path, and the complete raster and
encoded output never coexist in memory.
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
