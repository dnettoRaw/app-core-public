# Adaptive table pagination (opt-in, 1.0.0-rc.2)

Release candidate `1.0.0-rc.2` includes the variable-content, intrinsic-height, adaptive-pagination, diagnostics, paint-order and PDF Unicode changes below. Earlier source-only status notes describe development stages. Select the exact registry package after verification; external consumer migration remains separate.

`AdaptiveLayoutPlan::parse_yaml` reads a separate strict `filemaker_layout: '1.0'`
YAML design. Call `LayoutEngine::resolve_adaptive(&document, &plan)` after the
existing V1 compile/bind pipeline. This additive API preserves existing public
struct literals, V1 parsing, `resolve` and `resolve_intrinsic`. No dependency,
wire/storage version or application-specific rule is added.

The plan selects one root table, its `first`, `intermediate`, `last` and
`single` vertical bodies, a final root `closing` element, its gap and the
required `oversized_group: error` or `split_rows` policy. Body offsets/heights
are relative to the complete table box, with fixed width. All four bodies must
fit inside that box and the table box must fit the useful page region.

The declared bodies are BEFORE the closing reservation. The visible closing
height is computed with the existing intrinsic sizing and constraints. Its
height plus gap is subtracted exactly once from `single` or `last`. The table
header is then subtracted exactly once by the existing paginator. Hidden
closing blocks reserve neither height nor gap. There is no automatic reservation
for decorative page bands; their existing V1 semantics remain unchanged.

The existing pending group provides final-group lookahead. Only its final row
carries the difference between ordinary and final-page capacities. If that row
or group moves, its reserve and page-sensitive measurements are recalculated
for the destination. The final fragment is then translated into the selected
last/single body, and existing anchors place the closing block after its bottom.
No guessed last-page count or global fixed-point loop is needed for this
restricted vertical-table problem. Column sampling and group measurement may
still visit rows more than once; this is not a one-read streaming guarantee.

Equal contiguous `keep_together_by` keys remain together when the group and
its final reserve fit a destination. `split_rows` falls back between rows when
that combined requirement cannot fit; `error` rejects it. One indivisible row
plus reserve that cannot fit fails under either policy. A group that would
leave a completely empty first page fails explicitly, rather than returning
an empty page or silently reclassifying a continuation as a first page.
An empty dataset may render its header and closing, but the header must fit.

Current boundaries are explicit: one visible root table beginning on physical
page zero; one final body block anchored by `top: table-id.bottom+gap`; no
transforms/regions/keep-with-next on those two roots. The closing may be an
automatic text or flow group. Its descendants must remain inside its measured
box on the final table page. A conflicting collision or changing fixed-text
height produces an error with no partial returned scene. No table `page_bodies`,
conditional `reserve_after`, or built-in `total_fields` may be combined with
this reservation owner. Bind totals in the closing block as application data.
Multiple tables, wider/narrower final regions and arbitrary page-dependent
whole-document flow require a later explicit contract, not inferred behavior.

Resource limits still bound template bytes, nesting, elements, rows, pages and
collision work. Cancellation is checked before planning, during row visitation,
measurement and reservation evaluation. Individual shaping calls remain
cooperative boundaries. The existing spatial index is unchanged; no speed or
RSS gain is claimed. Preflight layout and export separately; successful export
or zero declared export losses does not prove geometry or text-layer fidelity.
The earlier ligature-extraction finding is corrected in the 1.0.0-rc.2 exporter; full clusters and context-specific `/ActualText` preserve logical text. Continue independent text-extraction and visual checks.

Run `cargo run -p appcore-filemaker --example adaptive_pagination --locked`.
`examples/adaptive-pagination.yml`, `adaptive-pagination-plan.yml` and separate
typed JSON datasets generate 1/2/5/20-page examples in `target/filemaker-examples/`.
The example enforces layout audit and export preflight before writing each PDF.
The 8-row regression falls from 3 pages to 2 with one closing reservation;
source row indices remain ordered and unique. No consumer measures text or
patches YAML strings in Rust.

Migration: retain the original template, move its body design to the plan,
remove the competing V1/row reservations, declare the closing anchor and select
the new layout call. Rollback restores that original template and layout call;
input data and persisted formats do not change. Included in `1.0.0-rc.2`; the external application has not been migrated.

`first_after: {element: title, gap: 8pt}` optionally moves the start of both
`first` and `single` below an earlier measured layout box on physical page zero.
Their declared bottoms are preserved; the start is never moved above the
original offset. This links an automatic two-column header and title to the
first table region without caller-side coordinate arithmetic. The table's
complete box may overlap those earlier boxes while its selected content body
does not: declare explicit local `collision.ignore` IDs in the template, as in
`examples/adaptive-header.yml`. No global collision disable is necessary.
An unresolved/later/other-page target or exhausted first body fails explicitly.
Non-empty datasets must place rows on the first page; an initial deferral that
would leave only headings is rejected rather than returning an orphan title.

Run `cargo run -p appcore-filemaker --example adaptive_header --locked` for the
short/long header workflow. The shared YAML and plan bind separate synthetic
JSON data; both layout audit and export preflight are enforced. Main pagination
and header examples together produce six PDF documents, for visual inspection.

## Bounded pagination evidence (1.0.0-rc.2 addition)

Use `LayoutEngine::diagnose_adaptive(&document, &plan, max_decisions)` instead
of `resolve_adaptive` when an explanation is needed. `AdaptiveLayoutReport`
exposes `outcome()`, `decisions()` and `into_outcome()`. Inspect or save decisions
before propagating the result: earlier evidence remains available on failure.
Each call owns independent evidence. Existing entrypoints and public struct
literals are unchanged; ordinary resolution creates no recorder.

Each `PaginationDecision` records the table ID, zero-based physical candidate
page, inclusive source row ordinals, the actual rule and numeric heights:
`capacity` excludes the table header once; `used_height` includes prior rows
and their reservations; `available_height = capacity - used_height`;
`required_height = group_height + reserved_height`. `final_reservation` is
part of `reserved_height`, never an additional amount to add. It combines the
closing block and gap with the difference between normal and final body heights;
it is not necessarily just the closing block height.

Rules are `fit`, `move_group`, `split_rows`, `move_row`, `skip_first_page` and
`reject`. Move records describe the source candidate before advancing; a later
fit uses destination measurements and reservation. Oversize policy decisions
assess an empty continuation candidate, even if that page is never emitted.
Records are execution evidence, not a list of emitted pages or a successful
export guarantee. Validation before pagination can fail with no decisions;
a rejection after pagination may retain earlier fit decisions but no scene.
An empty dataset has no row decisions. The rule enum is non-exhaustive.

The explicit record limit must be between one and `ResourceLimits.max_elements`.
Exhaustion returns `LimitExceeded`, preserving the prefix, without silently
truncating or returning a partial scene. Records retain IDs, row ordinals and
geometry, not row text, payloads or grouping values. The opt-in collector uses
short bounded mutex sections and keeps engine Send/Sync characteristics; it adds
allocation/serialization cost and is not enabled by default. Cancellation and
existing page, row and measurement bounds still apply.

`cargo run -p appcore-filemaker --example adaptive_pagination --locked` writes
`*-pagination.json`, `*-debug.json` and per-page `*-mask-*.svg` alongside the
1/2/5/20-page outputs. It reuses `DebugOverlay::build_bounded` (safe areas,
regions, layout/intrinsic/visual boxes and anchors) and the existing
`CollisionMask` SVG exporter. Masks describe geometric occupancy; use
`audit_layout` for effective overlap policies. `SceneInspector::explain_layout`
provides final element provenance, while `audit_layout` identifies collision
and clipping geometry. Continue strict preflight and rendered-PDF review:
zero exporter losses do not prove that layout is safe. Debug overlay labels
can contain source expressions; review them before external sharing.

Rollback: call `resolve_adaptive` again and stop saving diagnostic outputs.
No YAML, persistent format, dependency, spatial index or consumer migration.

`header_height` records the applied header reservation; `body_height = capacity + header_height` exposes its normal body before that subtraction.
