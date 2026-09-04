# appcore-filemaker-ai guide

This optional crate adapts deterministic `appcore-filemaker` sessions to the
bounded tool contracts accepted by `appcore-ai`. It does not add AI behavior to
the compiler and never lets a model choose filesystem output.

Create `FileMakerAiSession` with explicit `ResourceLimits`, fonts, optional
assets, and `AiBridgePolicy`. The policy bounds tool calls, patch operations,
JSON argument bytes, and serialized result bytes. Template `ai.editable` and
`ai.locked` lists are enforced across destructive subtrees before an atomic
patch changes the document. Textual purpose/rules are compact model context;
the deterministic bridge does not pretend to interpret natural-language rules.
Result sizing serializes into a bounded counter that retains no payload and
aborts as soon as `max_result_bytes` would be exceeded, avoiding a second full
JSON allocation while preserving the exact byte boundary.

Use `tool_definitions()` in `AiGenerationOptions`, then pass exact tool calls to
`execute_call`. Query tools are read-only. Mutation tools increment the session
revision only after a bounded candidate validates and, for graphical models,
resolves successfully.
Patch sequence is exactly the next revision, and the effective patch-operation
cap cannot exceed core `ResourceLimits`. Export returns bounded base64 in
memory.

`filemaker_export` accepts PDF, SVG, PNG, JPEG, HTML, and CSV. CSV selects one
bound table (or requires its exact ID when several exist) and streams the
bounded rows directly from dataset IR. Dataset sessions do not invent a page;
preview, masks, free regions, and graphical preflight still require a
document/canvas scene.

Every tool declaration has a closed schema matching its accepted arguments;
unknown fields fail. Capabilities expose remaining calls and a compact document
context. `load` cannot replace a trusted document and its AI policy unless the
host opts into `allow_document_replacement`; it is false by default.

`filemaker_schema` reports typed colors and every cascade layer. The bounded
`filemaker_set`/patch boundary accepts transactional `set_style`; export style
overrides remain paint-only and cannot alter resolved geometry.

`filemaker_add` accepts the compact strict source element when the object has a
`type` field, including source lengths, semantic paths, style, transform,
layer, and collision. A complete `ElementIr` with `kind` remains accepted.
The schema advertises Canvas units, primitives, path commands, and prepared
advanced graphics so a model does not need to invent pixel-paint operations.

`filemaker_inspect` accepts either an element ID or a page. Its structured
trace and `filemaker_explain` retain source geometry, anchors, region,
measurement, collision, page/reflow, and provenance. `filemaker_debug_mask`
declares page plus collision/layout/visual/combined view inputs;
`filemaker_query_free_regions` declares its bounded minimum dimensions.

Capabilities expose editable, flattened, and hybrid PDF and name the remaining
prepared PDF features separately. Hybrid paints deterministic outlines and an
invisible subsetted Unicode layer for search, selection, and extraction. Export
self-description guarantees caller-owned writer or bounded bytes,
strict/best-effort loss reporting, raster-only DPI, deterministic PDF metadata,
and PDF glyph subsetting; a model must not infer unavailable output.

`filemaker_validate` returns bounded layout issues and explicit truncation.
`filemaker_preflight` declares format/fidelity/mode/page/DPI plus strict and
accessibility policy in its tool schema. Discovery names schema, data, layout,
and preflight stages, complete fingerprint inputs, and resolve-on-miss caching.

Debug-mask and free-region tools pass the session's core limits into bounded
diagnostic geometry. Tool execution therefore cannot bypass the scene's
comparison or retained-geometry budget.

The session commits the immutable document and its resolved scene together.
Read-only tools clone only the scene `Arc`; they do not rerun layout. A patch
builds and validates one candidate, then atomically replaces both values, so a
failed edit retains the prior document and geometry.
