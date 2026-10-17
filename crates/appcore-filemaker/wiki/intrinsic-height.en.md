# Intrinsic height (opt-in, 1.0.0-rc.2)

Release candidate `1.0.0-rc.2` includes the variable-content, intrinsic-height, adaptive-pagination, diagnostics, paint-order and PDF Unicode changes below. Earlier source-only status notes describe development stages. Select the exact registry package after verification; external consumer migration remains separate.

Call `LayoutEngine::resolve_intrinsic(&document)` instead of `resolve` to give
explicit YAML `height: auto` content-based sizing. Existing `resolve`, V1
parsing, public struct literals and fixed-height overflow policies stay intact.
No new dependency or consumer-side measurement is required.

Text uses the existing shaping/measurement path, registered font, resolved
style, width, inline/block padding and line advances. Horizontal flow groups
use the largest child height; vertical flows sum heights and declared gaps.
`constraints: {min_height: 14mm}` retains the short design. Existing anchors
and flow see the measured box before collision placement. Input IR is unchanged.
A content height above `max_height` fails with the element ID instead of clipping.

This first opt-in supports definite widths and start-distributed body flows.
Automatic text accepts horizontal `wrap`, `expand` or `error`, without
`max_lines`; use a fixed height to explicitly select `clip`, `ellipsis` or
`shrink`. Automatic groups are indivisible and must fit one useful page.
Inside an automatic group, children must have no explicit x/y, anchors,
region, transforms, non-start geometry alignment, keep-with-next or
height-relative constraints. An expanding text child must itself use auto
height. Absolute/distributed automatic groups, vertical writing, automatic
page bands, aspect ratio/preferred height on auto nodes and percentage block
padding on automatic text fail explicitly. These restrictions avoid circular
measurement and incorrect estimates; they are not implicit fallbacks.

The walk checks cancellation, element/text limits and a maximum nesting depth
of 64 before cloning the bound document. Measurement checks cancellation per
node; shaping one node is not preemptible. The clone and measurement add work
and memory; no throughput or RSS improvement is claimed. Use layout preflight
on the final scene, including fixed elements and intentional overlays. Do not
reuse ordinary-layout scene cache keys for intrinsic resolution.

Run `cargo run -p appcore-filemaker --example intrinsic_height --locked`.
The shared `examples/intrinsic-height.yml` binds separate short/long JSON data,
keeps two columns and right-aligned text, and moves the title/body through
anchors. Outputs are under `target/filemaker-examples/`. No YAML coordinates
are substituted in Rust. The example enforces layout diagnostics before PDF
export. A successful export alone is not layout approval.

Migration is opt-in at the layout call, with auto heights/minimum heights in
the YAML. Keep the original design and call `resolve` for rollback. This is an
additive Rust API; it introduces no alternative YAML parser/version. Adaptive
first/intermediate/last/single table bodies and final reservations now use the separate adaptive plan; arbitrary global flow is outside that contract.

Within a root participating in intrinsic planning, containing groups are
structural: descendant collision policies explicitly ignore their ancestor
IDs. Sibling and external collision rules remain enabled. Ordinary resolution
and roots without auto heights retain their existing policies.

The earlier ligature-extraction finding is corrected in the 1.0.0-rc.2 exporter; full clusters and context-specific `/ActualText` preserve logical text. Continue independent text-extraction and visual checks.
