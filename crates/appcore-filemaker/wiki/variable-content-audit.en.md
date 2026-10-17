# Variable content: diagnosis and incremental plan

> Source update: increment 1 now implements FMV-01/02/03 and keep-block padding corrections, plus transformed-region and reflowed-margin regressions. The findings below describe the original audited baseline, not the corrected 1.0.0-rc.2. No package was published and the consumer is unchanged. Increment 2 adds opt-in intrinsic body containers; increment 3 adds adaptive table bodies and a final closing reserve. Increment 4 adds opt-in bounded pagination evidence, preserving records on failure and reusing debug overlays and SVG masks.

Date: 2026-10-17. Audited source: `appcore-filemaker 1.0.0-rc.1`.
Status: diagnosis before implementation, not a claim that the findings are fixed.
The original audit below is retained as baseline evidence. Current unpublished corrections are described above; the published package and consuming application remain unchanged.
[Português](variable-content-audit.pt.md) | [Français](variable-content-audit.fr.md)

## Reuse existing capabilities

| Requirement | Existing contract | Boundary |
|---|---|---|
| Export-equivalent measurement | `TextEngine`, `FontManager`, `measure_content`, `TextLayout` | Already shares explicit fonts, shaping, width, padding and line height. Do not build another measurer. |
| Growing text | `text_options.overflow: expand` | Grows a text element, not its parent container; see FMV-01 for line caps. |
| Dependent placement | Vertical/horizontal flow and earlier-element anchors | Parent bounds do not measure the maximum height of their columns. |
| Keep content together | `keep_together_by`, `keep_with_next` | Keys must be contiguous. Oversized groups split at row/element boundaries; an oversized indivisible row fails. `group_by` is not a keep rule. |
| Different bodies | `table.page_bodies.first` / `continuation` | No `last` body. Unknown YAML fields fail closed. |
| Page bands | `page.master`, `first`, `continuation`, `last` | Painted after body pagination; they do not automatically reserve space. Existing `last` excludes one-page documents. |
| Intentional overlap | Local `overlay`, `group`, `collides_with`, `ignore` | Global collision disabling is unnecessary; audit inconsistency FMV-03 needs correction. |
| Geometry and diagnostics | Layout/intrinsic/visual bounds, `SceneInspector`, `LayoutTrace`, `audit_layout`, `preflight`, `DebugOverlay`, `CollisionMask` | Reuse these; detailed per-break capacity/reservation evidence is missing. |
| Bounded execution | Fixed-point geometry, resource/comparison/reflow limits, cancellation | Global pagination convergence will need its own bounded work budget. |

See [measurement](../src/layout_measure.rs), [text](../src/text.rs),
[flow](../src/layout.rs), [groups](../src/table_group_pagination.rs),
[table regions](../src/ir.rs), [page layers](../src/layout_page.rs),
[collision](../src/collision.rs), [audit](../src/validation_layout.rs) and
[debug](../src/debug.rs).

## Consumer configuration versus engine defects

The supplied template and adapter were read without using actual document data.
Both header columns declare 14mm, four lines, a 9pt font and 1.25 line height.
Four line advances total 45pt, approximately 15.875mm: even four lines may
exceed that box. `wrap` does not request growth; `min_font_size` does not enable
`shrink`. Absolute title/body coordinates do not follow a larger text box.
The adapter already measures both columns, takes their maximum and patches YAML
strings. This is a fragile workaround for a genuine declarative limitation.

The declared table is 267mm high, but both usable bodies end at page coordinate
260mm: `16 + 70 + 174` first, `16 + 244` continuation. The last 23mm of the table
box are excluded on every page, independent of whether closing content is
painted there. Closing elements follow the table; the engine does not infer
these reservations. `safe` is not `margin` or an automatic body reservation.
Grouping is already configured with `keep_together_by: layout_group`; without
representative input, correct contiguous assignment of description/comment keys
is unverified. Do not blame the grouping engine without that evidence.

The consumer disables collisions globally and calls `export_bytes`, returning
format losses without `preflight` or `audit_layout`. Zero format loss is not
layout safety. Existing APIs can improve that integration without a new API.

## Reproducible findings

| ID | Fixture | Observed result | Classification |
|---|---|---|---|
| FMV-01 | [expand-cap-silent.yml](audit/expand-cap-silent.yml) | Six lines become four with `expand` and `max_lines`; no truncation diagnostic, zero PDF losses and a clean audit. | Engine defect: missing loss evidence. Preserve explicit cap intent rather than silently redefining every cap. |
| FMV-02 | [next-page-margin.yml](audit/next-page-margin.yml) | `next_page` starts page two at `y=0` despite a 20pt top margin; audit accepts it. | Reflow defect plus an audit gap: physical-page checks are insufficient. |
| FMV-03 | [allowed-groups.yml](audit/allowed-groups.yml) | Placement accepts two non-interacting groups; audit reports one collision, without visual overflow. | Audit false positive: apply effective `CollisionRule::applies_to`. |
| FMV-04 | [auto-container.yml](audit/auto-container.yml) | `height: auto` retains its fallback height; an anchored sibling crosses the taller column. | Contract limitation: current auto is not child-driven intrinsic sizing. Do not change published V1 semantics silently. |
| FMV-05 | [configured-cap.yml](audit/configured-cap.yml) | Clipping is diagnosed while PDF format losses remain zero. | Explicit configuration and missing preflight integration, not a shaping defect. |

Collision continuation compares against physical height and resets to zero for
both bottom overflow and `next_page`. A correction must also cover `push`,
regions and intentional page bands. Keep-block measurement uses
`TextLayout.measured.height` while expanded placement uses `outer_measured()`;
reproduce the padding inconsistency before calling it a confirmed regression.
Existing explanations lack remaining capacity, next-group height, applied
reservations and the deciding rule for every break. Diagnostics must stay
bounded and omit document content.

## Evidence and limits

55 existing tests passed across `reflow`, `layout`, `table`, `pages`,
`debug_inspect` and `text`. Nine synthetic scenarios were executed. Hybrid PDFs
were rasterized using Poppler and visually inspected.
[The matrix](audit/matrix.yml) with [1](audit/matrix-1.data.json),
[2](audit/matrix-2.data.json), [5](audit/matrix-5.data.json) and
[20](audit/matrix-20.data.json)-page inputs produces those exact page counts,
six rows per page, with identity/description/comment in each row. Source indices
remain exactly `0..N`, without loss, duplication or reordering. These short,
fixed-height controls do not qualify adaptive sizing, last-page convergence,
extreme descriptions, mixed rates, detailed discounts, strikethrough prices,
totals, barcodes and watermarks together.

Local evidence is under `target/appcore/filemaker-layout-audit/`: PDFs, PNGs,
scenes, masks, audit JSON and logs. These are diagnostic artifacts, not tests
that permanently enshrine defects. The linear spatial index was not replaced;
release performance was not measured and no performance gain is claimed.

## Incremental implementation gates

1. **Compatible corrections.** Add desired-behavior regressions for FMV-01/02/03,
   run them red before each fix, and reproduce keep-block padding. Preserve
   public structs, existing YAML and serialized values. Test push, next-page,
   shrink, error, overlay, anchors and resize with regions, margins, transforms
   and page bands. Benchmark the current index in optimized sparse/dense cases
   before considering replacement. Gate: complete FileMaker suite,
   fmt/check/Clippy, rustdoc, consumers and an appropriate SemVer baseline.
2. **Opt-in intrinsic sizing.** Reuse shaping; vertical columns sum heights and
   gaps, horizontal columns take the maximum height. Publish anchors and advance
   flow only after measurement. Define width, padding, line height, empty text,
   min/max and indivisible overflow. Reject cyclic percentage-height dependencies.
   Preserve the short header's 14mm as an explicit minimum. Define clip, grow,
   shrink and error without silent loss. Adding even defaulted public fields
   breaks Rust literals: propose separate opt-in types/entry points with
   migration and rollback instead of silently changing V1 `height: auto`.
   Crate versions remain independently owned RCs. Gate: identical short layout,
   complete long columns and YAML-owned title/body placement without Rust patches.
3. **Role-specific bodies and convergence.** Reuse table pagination and grouping;
   separate physical/content/paint/reservation geometry. Reserve each active band
   exactly once. Define single-page behavior explicitly without changing existing
   `PageRole::Last`. Oversized groups explicitly split between rows or fail.
   Recompute when final-page assignment changes, using stable order, fixed-point
   units, cancellation and global pass/measurement budgets. Detect repeated
   assignments/reservations; fail clearly rather than manufacturing blank pages
   or double reservations. Gate: 1/2/5/20 pages, last-page boundary cases, no
   orphan headings, empty retries, loss/reordering or inactive reservations.
4. **Explanations and safety.** Reuse audit/preflight/inspection/overlays. Add an
   opt-in bounded break report containing page, IDs/source indices, useful region,
   available height, group height, reservations, rule and before/after geometry.
   Preserve existing report contracts; use a separate versioned surface where
   needed. Gate: deterministic JSON, fail-closed truncation, useful cycle/budget
   errors and format export kept distinct from layout safety.
5. **Full visual corpus and adoption.** Synthetic long text, grouped content,
   mixed rates, detailed discounts, crossed prices, totals, EAN/QR, stamps and
   watermarks. Fiscal calculations stay in application data. Regenerate/render
   every PDF page and compare scene geometry, pixels and content order; check
   HTML/SVG/raster from the same scene. Only then replace the consumer's YAML
   string workaround, retain rollback and prove registry-package equivalence.
   Final gate: all requested acceptance criteria, EN/PT/FR docs and a qualified
   independent RC. This audit does not publish or deploy a package.

## Reproduce with the existing CLI

Run from the workspace root. This command passed with zero issues for the 20-page control. Passing it on a defect fixture does not prove safety; compare the findings above.

```bash
cargo run -p appcore-filemaker-cli --locked -- \
  --json --strict \
  --font Body=crates/appcore-filemaker/examples/assets/NotoSans-Regular.ttf \
  --data crates/appcore-filemaker/wiki/audit/matrix-20.data.json \
  preflight crates/appcore-filemaker/wiki/audit/matrix.yml \
  --format pdf --pdf-mode hybrid
```

## Increment 1 validation — 2026-10-17

Unpublished compatible source corrections are implemented. Eleven new tests
cover truncation, fragment diagnostics, keep-block padding, margin/region
continuation, parent transforms, imported reflowed body validation, overlay paint
order, footer painting and shrink anchors. Failures were retained before each
correction; two old reliability fixtures now explicitly model unapproved imported
overlap instead of permitted overlays. The complete FileMaker suite passes
236 tests; AI/CLI consumers pass 47. Workspace check/Clippy, rustdoc, norm and
formatting pass. Against published `1.0.0-rc.1`, patch SemVer checks pass 223
applicable checks (30 skipped). No public fields or wire values were added.

The regenerated 1/2/5/20-page control PDFs match the baseline byte-for-byte;
source indices and extracted PDF identities remain ordered and complete.
Release-mode local baseline: `reflow_dense_64` averages 1.412ms/iteration and
`diagnostic_geometry_256` 1.852ms/iteration, ten iterations each. These are
different workloads, not a speedup comparison, percentile study or production
budget. The linear index is unchanged. Evidence is retained under
`target/appcore/filemaker-compatible-fixes/`. Later intrinsic-sizing,
last-page-convergence and full commercial-corpus gates remain open.

## Intrinsic height (opt-in, 1.0.0-rc.2)

[Intrinsic height (opt-in, 1.0.0-rc.2)](intrinsic-height.en.md). `LayoutEngine::resolve_intrinsic` — `examples/intrinsic-height.yml`.

## Adaptive table pagination (opt-in, 1.0.0-rc.2)

[Adaptive table pagination (opt-in, 1.0.0-rc.2)](adaptive-pagination.en.md). `AdaptiveLayoutPlan` / `LayoutEngine::resolve_adaptive`.

Opt-in bounded pagination evidence: `diagnose_adaptive` returns numeric decisions and preserves earlier records on error. See the adaptive pagination guide.

[Mixed synthetic corpus and consumer adoption gate](variable-content-corpus.en.md).
