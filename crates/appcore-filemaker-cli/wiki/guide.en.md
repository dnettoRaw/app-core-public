# appcore-filemaker-cli guide

This bounded process adapter compiles the same strict YAML and uses the same
resolved scene as the Rust API. Export format is selected only by the command,
never by template input.

Use `check` for schema validation, `validate` for bound layout, `preflight` for
one exporter request, and `render` for atomic PDF, SVG, PNG, JPEG, HTML, or
table CSV output. `inspect`, `explain`, `free-regions`, `debug`, and `mask` are
diagnostic boundaries.
`schema` and
`capabilities` are read-only. `migrate` is reserved and returns unavailable
without changing input.

`schema --json` exposes typed colors, the complete style cascade, paint-only
export overrides, and the fact that layer/z-index never control collision.
It also lists semantic Canvas coordinate units, primitives, path commands, and
prepared advanced graphics; templates never encode a pixel-painted surface.

Use `debug TEMPLATE --grid 1|5|10|20 --view combined` for the complete
non-mutating overlay. `mask` exports collision/layout/visual/combined geometry
as PNG, PDF, SVG, or JSON. The JSON separates occupied, free, collisions, and
overflow. `inspect` and `explain` return retained source geometry, anchors,
region, measurement, collision, page/reflow, and provenance.
`free-regions TEMPLATE --minimum-width 20pt --minimum-height 10pt` returns the
bounded resolved rectangles that can contain that minimum size.

`capabilities --json` exposes editable, flattened, and hybrid PDF. Hybrid draws
deterministic vector outlines and adds invisible subsetted Unicode text for
search, selection, and extraction. WebP, XLSX, ZPL, ESC/POS, PDF/A, links,
bookmarks, and tagged accessibility remain prepared.
Exporter self-description covers writer/bounded bytes, strict/best-effort loss
reports, raster-only DPI, deterministic PDF metadata, and font subsets.

Pass data with `--data`, fonts with repeatable `--font NAME=FILE`, their exact
fallback order with repeatable `--font-fallback NAME`, and an explicit sandbox
using `--assets-root`. Apply ordered patch files with repeatable `--patch FILE`.
For CSV use `render TEMPLATE --format csv --table ELEMENT --output FILE`; one
table may omit `--table`. Use `--json` for stable automation responses and
preserve nonzero exit codes.

Every command emits concise human text by default and stable JSON with
`--json`. Capability discovery publishes exit codes 0 (success), 2
(validation), 64 (usage), 65 (data), 66 (missing input), 69 (unavailable), 70
(software), 73 (cannot create), 74 (I/O), 75 (temporary resource failure), and
130 (cancelled).
Both modes include one trailing newline and share a 512 MiB stdout ceiling.
Pretty JSON is sized first and serialized directly through a fixed 16 KiB
buffer, so automation output never requires a second complete string.

Artifact writes use exclusive temporary files, data sync, and atomic rename.
`render` and `mask` reject an output path resolving to their input template.
`migrate` is unavailable and non-mutating; a future migration cannot write
without a new explicit flag and contract.

`check`, `validate`, and `preflight` keep schema, resolved-layout, and
exporter-aware diagnostics separate. JSON includes bounded issues and explicit
`truncated`; strict rejects warnings and truncation always fails closed.
`schema --json` also lists typed-data validation, complete fingerprint inputs,
and bounded immutable resolve-on-miss caching.

Template, data, and font reads stay on one opened handle and stop after
`limit + 1` bytes. Debug overlays and masks reuse the command's core limits,
including the diagnostic comparison and retained-geometry budget.
