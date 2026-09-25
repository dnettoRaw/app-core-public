# appcore-filemaker guide

Diagnostic messages are shortened only at UTF-8 boundaries: errors retain at
most 1,024 bytes; source paths, validation issue messages and export loss
messages retain at most 512. Oversized backing buffers are replaced with the
bounded prefix. This bounds retained diagnostic text, not the caller's prior
allocation, allocator overhead or the number of accumulated reports.

Run `cargo test -p appcore-filemaker --test reflow` for the exact push-attempt
boundary, negative-gap rejection and shrink minimum-size failure. A reflow
limit error is not evidence that the repeated-state cycle detector ran.

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

Start with the public
[step-by-step YAML guide](https://wiki.appcore.dnettoraw.com/crates/appcore-filemaker-yaml).
It builds a strict V1 template incrementally and provides the complete accepted
top-level and element-field reference. Keep `appcore-filemaker schema --json`
as the executable source of truth for the installed binary.

Then compare the runnable [basic example](examples/basic.en.md) and
[intermediate example](examples/intermediate.en.md). The
[architecture and contract reference](architecture.en.md) explains the engine
boundaries.

Page layers are traversed lazily for each physical page, so resolving role
layers does not allocate a temporary list of element references.
Distributed flow planning applies the same allocation-free visible-child pass
when calculating spacing.
Fingerprinting also sorts borrowed asset names, so deterministic resolution does
not clone each name.

Register exact font bytes and an ordered fallback list before measurement; the
order is fingerprinted and exporters embed the families actually selected in
resolved glyph runs. Apply runtime patches at bind time, before layout, so text
measurement, collision, pagination, and exports all consume fresh geometry.
Fingerprint JSON uses a sizing pass followed by direct hashing under the
aggregate `max_output_bytes` budget. It preserves the exact V1 framing without
retaining the canonical JSON bytes.

For vertical Japanese or similar layouts, set
`text_options.writing_mode: vertical`. The engine wraps against element height,
shapes each column top to bottom, and advances columns right to left. Keep
`horizontal` (the default) for normal horizontal and BiDi text.

## Raster memory ownership

Dense raster cases `raster_png_dense_rows_8` and `raster_png_dense_rows_256`
render 4,096 rectangles plus a background. Collision is disabled in that fixture
because these cases isolate export, not reflow. Four `raster_candidates_*`
cases separately compare linear selection with an experimental strip index
at 8/256 rows. The index is benchmark-only, capped at 65,536 memberships;
building and dropping it are timed. Exact candidate lists/order are checked
outside timing; counts/checksums are checked inside. The model covers one
96-DPI page and mirrors antialias padding, not general production indexing.

The runtime benchmark compares PNG/JPEG strips of 8, 64 and 256 rows on the
same resolved FHD scene (background plus 256 rectangles), writing to a sink.
Cases are `raster_png_rows_8/64/256` and `raster_jpeg_rows_8/64/256` (one
numeric suffix per case). Compile/bind/layout are outside timing; export
validation, rasterization, encoding and outcome checks are inside. This
measures strip trade-offs, not an element-index implementation or native heap.

For PNG/JPEG, `export_raster_controlled` accepts `RasterOptions::new(bytes,
rows)` without changing `ExportRequest`. Defaults remain 4 MiB and 256 rows;
accepted limits are 1 byte–64 MiB and 1–4096 rows, with a separate 4 MiB
scanline ceiling. A scanline that cannot fit is rejected before encoding.
For example, `RasterOptions::new(1024 * 1024, 64)?` caps the working surface
at 1 MiB/64 rows. Smaller strips can require more repeated rendering, especially
for JPEG block traversal. Non-raster formats are rejected; layout, loss reporting
and paint overrides remain shared. Existing exports, CLI/AI and debug masks use
the defaults. This bounds the surface, not codec/assets/output or process RSS.

JPEG drops the previous cached strip before rendering its replacement, so
cache turnover does not retain two strip surfaces. A failed replacement leaves
the cache empty and preserves the rendering error. PNG streams strips without
collecting the full surface. Codec scratch, decoded assets and caller-owned
output buffers are additional memory; the strip ceiling is not a process RSS
limit. No RSS reduction was measured for this cache-lifetime correction.

Internal encoders reject zero dimensions and zero strip height before writing
or invoking a renderer. The strip renderer checks its planned row ceiling
before allocating. These are defensive boundaries; public export validation
still applies before this layer.

After resolving a scene, call `audit_layout` with explicit resource limits and
`LayoutSafetyOptions`. The report is bounded, exposes overflow/collision/text
counts, and can reject warnings in strict mode. Its JSON representation is
stable enough for golden fixtures and support evidence; export still owns the
final format-specific preflight.
