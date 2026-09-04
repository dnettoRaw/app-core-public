# Architecture

`appcore-filemaker` compiles `Template + Data + Patches` into a typed IR,
measures explicit assets and fonts, resolves layout/collision/reflow, and emits
an immutable scene. Inspection, preflight, and exporters consume that scene;
they cannot change geometry.

Geometry uses signed fixed-point microunits. YAML accepts only
`filemaker: "1.0"`. Includes, assets, and fonts use explicit sandboxed
resolvers. Every parser, loop, dataset, raster, and output path is bounded by
`ResourceLimits`.

Binding uses one shared element counter across roots, descendants, and repeat
expansion, with cooperative cancellation/progress at element boundaries.
Collision lookup has its own total comparison budget in addition to the reflow
limit, so adversarial sparse or overlapping scenes fail closed instead of
performing unbounded quadratic work. Filesystem assets are opened from a
canonical root without following a substituted final symlink/reparse point,
read through the caller's byte cap, and sandbox-revalidated around the read.
Export cancellation is checked before any caller-visible bytes are written.

The core never depends on `appcore-ai`. The optional bridge translates 20
bounded tools into core operations, while the CLI uses `appcore-args` and
atomic artifact writes. Source contracts cover explicit themes/tokens,
computed data, semantic paths, page boxes, and image intent. The resolved
scene retains glyphs, path commands, image placement, distinct bounds,
provenance, and page metadata.

Canvas is a semantic drawing contract, not a pixel buffer. Coordinates accept
`pt`, `px`, `mm`, `cm`, `in`, `%`, logical `lu`, and bounded `0..=1`
`norm`/`normalized` values. Text, image, line, rect, circle, ellipse, polygon,
path, and group nodes keep their identity through IR and layout; path commands
are move, line, cubic curve, and close. Circles reject unequal resolved axes.
Safe areas, presets, layers/z-index, transforms, and collision remain explicit
orthogonal inputs to the same fixed-point scene.

Colors cross YAML and the IR without collapsing their color space. Sources may
use stable names, hex, integer `rgb`/`rgba`/`gray`, millionth-channel `cmyk`, or
a tagged typed value. Fill is the background paint, stroke plus stroke width is
the border, and opacity remains separate. Memory and canonical-root filesystem
resolvers implement the asset, template, and font boundaries with traversal
checks and caller-provided byte caps; registering a font never scans the OS.

The normative style order is executable rather than descriptive: engine
defaults, active theme, template, expanded component/named/inline style,
ordered conditional `style_rules` at data binding, transactional runtime
`SetStyle`, then `ExportStyleOverride`. Runtime changes precede measurement.
The export layer is paint-only (fill, stroke, opacity, text color), therefore
an exporter cannot change font metrics, stroke bounds, or layout. Layer and
z-index sort the immutable paint list and never influence collision decisions.

Image metadata is resolved once for both raster and SVG assets. Contain and
scale-down preserve aspect using fixed-point-unit ratios; fill, intrinsic-size
none, crop, focal cover, and optional EXIF orientation produce immutable source,
destination, and clip rectangles. Effective raster DPI is computed from the
transformed destination in preflight. SVG/HTML embed SVG assets; PDF/raster
report their unsupported SVG rasterization instead of dropping it silently.

Collision policy cascades deterministically from document to page, region,
group, and element. The boolean `collision: false` shorthand is fail-explicit,
and the spatial index consumes the selected measured layout, visual, or
intrinsic bounds before reflow.

Transforms are also resolved before the spatial query. Translation,
integer-degree rotation, fixed-point scale, flip/mirror, and explicit origins
compose through groups. The resolved matrix and its axis-aligned visual and
collision bounds are shared by PDF, SVG, raster, and HTML exporters.

Text layout intent crosses YAML, IR, measurement, and export without a renderer
reflow. `text_options.overflow` accepts `wrap`, `shrink`, `ellipsis`, `clip`,
`expand`, or `error`, alongside bounded `max_lines`, absolute
`min_font_size`, and fixed-point `line_height`. Expansion happens before the
spatial query; clipping becomes resolved geometry; SVG and HTML consume shaped
and truncated runs instead of the original literal. `writing_mode: vertical`
shapes top-to-bottom columns flowing right to left, and all graphical exporters
consume those resolved columns and runs. PDF and raster use the glyph advances
directly. Color emoji remains an explicit capability loss until an exporter
implements it.

Geometry constraints are resolved before measurement and collision.
`constraints` carries minimum, preferred, maximum, and a millionth
width/height aspect ratio; `align_x` and `align_y` select start, center, or end
inside the active page/region/group. Anchors may target an earlier element edge
or a named guide with `guide:name[+offset]`. Contradictory coordinates,
constraint ranges, and aspect ratios fail explicitly. A runtime move clears
anchors/alignment, and a runtime resize clears earlier size constraints.

Flow containers distribute fixed-size children with `start`, `center`, `end`,
`space_between`, `space_around`, or `space_evenly`. A non-start distribution
requires every visible child to have an explicit, preferred, or aspect-derived
primary size. Overflow and auto-measurement ambiguity are typed layout errors.

Top-level named `exclusions` are page-relative rectangles resolved into
fixed-point geometry before element placement. They are non-painted, must stay
inside the trim box, repeat on every physical page, and seed the spatial index
with an immovable highest-priority rule. Optional `group` and `collides_with`
fields use the same symmetric collision contract as elements. Existing
push/error/next-page/shrink policies remain responsible for reflow; repeated
instances share the global geometry budget. Inspection, collision masks, and
free-region queries retain the resolved exclusion geometry, while scene
exporters receive no paint node for it.

The strict page source accepts `master`, `first`, `continuation`, and `last`
layers. Each layer has explicit `background`, `header`, and `footer` element
lists. Master elements repeat on every page; one role layer is selected after
body pagination; and `{page}`/`{pages}` text is substituted only after the
bounded total is known. Page-layer elements share component, theme/style,
binding, patch, measurement, and exporter contracts with body elements, but
are forced into collision-free paint bands. Tables, repeat, and anchors to
other elements are rejected there so decoration cannot repaginate the body.
The resolved `collidable` flag keeps these overlays out of collision preflight,
collision masks, and free-region subtraction without removing their paint.

The table engine consumes restartable `Dataset` streams without materializing
the complete input. Fixed, bounded-sample auto, and weighted-flex columns
resolve to exact widths. Fixed or callback-measured row heights paginate with
first/repeating header capacity, group boundaries, ordered conditional styles,
and checked integer/decimal/currency totals emitted only on the final page.
Row count, field count, cell bytes, expression steps, samples, and pages are
explicit bounds.

Only the current bounded table page retains cloned source rows. The layout sink
turns that page into a `ResolvedTableFragment` immediately, so raw pages do not
accumulate beside the resolved scene. CSV borrows textual cells where possible
and writes quoting escapes in pieces rather than constructing a second cell.

The strict YAML frontend admits table intent only on `type: table` elements and
requires an array binding. Columns, grouping, totals, conditional styles,
header policy, and row sizing cross into `TableIr`; binding validates object
rows and retains typed values. Template limits can only tighten the compiler's
global row, field, and cell bounds.

Layout consumes that typed intent and emits one `ResolvedTableFragment` per
physical scene page. Final column widths, header repetition, row and cell
rectangles, data-rule styles, group continuity, totals geometry, and shaped
cell text are immutable exporter input. A continuation participates in normal
page limits and collision placement; renderers never measure or repaginate it.

PDF editable/flattened/hybrid, SVG, raster, and HTML now paint those fragments
directly. PDF font usage includes every cell run, SVG/HTML embed fonts selected
by data styles, and raster outlines the same glyphs. Semantic HTML retains
table, header, body, row, group, and footer meaning; fixed HTML uses the same
resolved dimensions. Preflight validates cell counts, bounds, diagnostics, and
embedded-PDF font availability for editable and hybrid modes.

Prepared capabilities remain explicit: strict fidelity returns
`FM-EXPORT-UNSUPPORTED`; best effort records the exact loss. No renderer makes
a silent approximation.

Debugging is derived only after layout. `DebugOverlay` supports exact
1/5/10/20-point grids, rulers, coordinates, IDs, distinct bounds, anchors,
resolved regions, safe areas, collision/exclusion geometry, and crosshairs,
without entering the scene paint list. Collision/layout/visual/combined masks
derive their own occupied and free rectangles and export PNG, PDF, SVG, or
stable occupied/free/collisions/overflow JSON. Each resolved element retains a
bounded trace of source x/y/width/height, anchors, region, proposed geometry,
measurement, inherited collision policy, page/reflow, and provenance for
structured inspection and deterministic explanations.
JSON, SVG, and PDF mask exports first count under `max_output_bytes` without
retaining the output, then serialize directly to the caller's writer. PDF uses
the shared object/xref emitter and an exact-length command stream rather than a
page or file buffer. This preserves pre-write rejection; the byte-returning JSON
convenience API pre-sizes its one exact result allocation.

Exporter options are format-scoped. DPI affects only PNG/JPEG and JPEG quality
only JPEG. PNG starts transparent and preserves alpha; JPEG composites on white
only after recording style or raster-image alpha loss. HTML declares semantic
capability only for semantic mode. PDF editable, flattened, and hybrid modes
share deterministic title/creator/producer metadata. Editable mode embeds exact
glyph subsets and Unicode maps. Hybrid paints the same deterministic outlines
as flattened mode, then places invisible subsetted Unicode text at the resolved
glyph coordinates for search, selection, and extraction. Every document format writes to a caller-owned
`Write` or bounded `export_bytes`; CSV streams rows and also offers bounded
bytes. Links, bookmarks, tagged accessibility, PDF/A, WebP, XLSX, ZPL,
and ESC/POS remain named prepared contracts rather than silent approximations.

Validation has four explicit boundaries: schema, typed data and bindings,
resolved layout, then exporter-aware preflight. Reports retain bounded warnings;
strict policy rejects warnings and truncation always fails closed. Preflight
predicts asset/vector, CMYK, JPEG alpha, effective-DPI, font-embedding, and
accessibility gaps in addition to glyph, overflow, and collision diagnostics.

The deterministic fingerprint frames schema/engine versions, canonical
template/data/patches, referenced asset digests, and registered font digests.
Canonical JSON fields run through a sizing writer and then directly through
SHA-256 under the aggregate `max_output_bytes` budget, preserving V1 framing
without a full JSON buffer.
`LayoutEngine::resolve_cached` resolves only on a bounded `SceneCache` miss,
shares immutable scenes for render-many, and rejects stale engine versions.
The cache is bounded by both entry count and aggregate serialized scene bytes.
`OperationLog::new_bounded` applies the same dual bound to `Arc<DocumentIr>`
snapshots; undo and redo move owned documents instead of cloning them again.
`BorrowedDataset` streams an existing row slice without duplicating it.

Raster composition uses vertical strips capped at 256 rows and about 4 MiB,
with a separate 4 MiB scanline ceiling. PNG streams those strips in scan order;
JPEG requests them through its documented 8-by-8 block traversal, and PNG
collision masks reuse the strip encoder. The renderer still consumes only
resolved geometry and never measures or resolves collision.
CSV streams rows. SVG and HTML perform a bounded counting pass and then write
markup, escaped text, paths, and base64 assets incrementally, preserving
limit rejection before touching the caller's writer. PDF applies the same
bounded sizing pattern, emits one independent `pdf-writer` object chunk at a
time, tracks offsets, and writes the classic xref/trailer last. It does not
retain a final document buffer; decoded images and font subsets are released
progressively.
Collision-mask SVG follows that counting/streaming path, escapes IDs in pieces,
and collision-mask JSON performs deterministic pretty-JSON sizing followed by
direct serialization. The `collision_mask_json_4m` workload measures an exact
4,188,826-byte mask with idle, peak, and retained RSS checkpoints.
Collision-mask PDF writes fixed-point commands directly into its declared
stream and then completes the classic xref. `collision_mask_pdf_100k` measures
100,000 rectangles and an exact 1,800,626-byte PDF under the same checkpoints.

Reliability gates retain exact SVG visual and collision-mask snapshots plus
fixed-point geometry property tests. Dedicated fuzz targets exercise the full
bounded YAML/bind/layout pipeline, arbitrary Unicode data and huge text,
corrupt raster assets, absurd geometry and overlaps, circular anchors, and
malformed, circular, or over-depth include graphs; malformed input may return
a typed error but must not panic, loop forever, or allocate without a bound.

The final public-scene boundary is guarded independently of compilation.
Export and preflight reject a stale engine version, malformed styles or image
placements, coordinate overflow, and page/element/path/row/text budget excess
before writing. Bounded overlay, mask derivation/JSON, collision, and
free-region APIs consume `max_preflight_comparisons`; convenience entry points
use bounded defaults. `ElementId` validates again during deserialization.
Controlled export checkpoints execute inside the actual renderer element loops,
so cancellation still prevents the staged artifact from reaching the caller.
The explicit-font pipeline uses maintained `harfrust` shaping and `skrifa`
validation, metrics, and outlines; the final audit removed the unmaintained
`rustybuzz` and `ttf-parser` dependencies without enabling OS discovery.
When a valid font omits OS/2 capital height, the PDF descriptor's named policy
uses ascent for `CapHeight`; missing glyph advances remain typed errors.

The runtime benchmark keeps focused compilation separate from the complete A4
workflow. `a4_report_end_to_end` and `a4_report_pdf_hybrid` decode the maintained
two-page YAML and data, apply a transactional patch, resolve
measurement/collision/reflow, run strict preflight, and stream editable or
hybrid PDF into a sink. `a4_report_export_matrix` reuses that complete pipeline
once per iteration and covers nine outputs: editable, flattened, and hybrid
PDF; SVG; semantic and fixed HTML; PNG; JPEG with explicit best-effort losses;
and dataset CSV. On Apple M1 it measured 70.56 ms p50, 71.34 ms p95, 0.22 ms
MAD, and 10.64 MiB peak RSS. `appcore-dev bench` samples each workload in
isolated processes, so its peak RSS is not confused with the smaller Canvas
compilation case.
