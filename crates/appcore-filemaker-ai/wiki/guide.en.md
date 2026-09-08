# appcore-filemaker-ai guide

JSON conversion errors retain at most 512 bytes without splitting UTF-8
characters or keeping an oversized message buffer. A malformed Unicode tool
argument returns a controlled error and preserves the document and revision;
the failed call still consumes its budget. Serde may have allocated its own
error message first; this is not a peak-allocation or secret-redaction guarantee.

`filemaker_validate` also counts its complete borrowed envelope before building
JSON, including issue messages, template escaping and truncation. Warnings
remain valid unless errors or truncation are present; a truncated report never
becomes `valid: true`. The core report is still built first within its issue
limit: this avoids a rejected JSON tree, not the diagnostic report itself.

Tool-name admission and allowed-tools policy validation use static argument
contracts, without rebuilding all public JSON schemas or retaining a global
schema cache. `tool_definitions()` copies its names, descriptions and exact
serialized schemas from static contracts when explicitly requested for model
discovery; it no longer builds intermediate JSON trees or tool-category vectors.
Unknown names are rejected before call accounting;
known tools retain the same policy, argument and call-budget checks.

`filemaker_capabilities` counts a borrowed view of document context, limits and
policy before building its JSON tree. Oversized purpose/rules/editable/locked
context is rejected without cloning those collections into JSON. Accepted
responses preserve all fields, ordering within lists and exact escaped-byte
accounting; an empty session still returns `document_context: null`. The
returned tree remains owned, and failed calls still consume the call budget.
This does not bound session residency, diagnostic construction or exporter scratch.

The `runtime` benchmark includes `create_patch_256_elements`: a fresh session
creates a 256-rectangle Canvas, hides one element through a tool and inspects
another, checking revisions and results. Compile/bind and JSON fixture creation
are outside timing; JSON parsing, typed conversion, two layouts, policy/result
checks and session teardown are inside. This measures an editing workflow, not
text shaping, exporters, cancellation latency or every tool.

Typed mutation arguments, source elements, lengths and export style overrides
are deserialized from the existing JSON tree without cloning that tree first.
The resulting IR/patch still owns its required strings and collections; this
is not zero-copy parsing of the original JSON input.

Mutation acknowledgements are sized before document/scene commit. If
`max_result_bytes` cannot hold the response, create/load/patch and derived edit
tools return a policy error without changing the document or revision. The
attempt still consumes the call budget; candidate validation may already have
run. This result budget is not a reservation for layout scratch memory.
The byte ceiling covers the complete serialized `ToolExecution`, including
`tool`, `revision` and `value`. Builders receive only the exact remaining value
budget, so an outer-envelope rejection cannot occur after a mutation commits.

`export_dataset_csv_controlled` accepts `OperationControl`, checks cancellation
before output and at row boundaries, and reports completed rows in the Export
phase. A cancelled export returns `Cancelled`; the writer can retain a partial
CSV prefix which the caller must discard or roll back. Dataset/writer callbacks
are cooperative, not preemptible. The AI bridge uses the same control for CSV.

Use `FileMakerAiSession::with_control(OperationControl)` to share cancellation
and progress with layout/reflow, validation/preflight and graphical export.
Install it on `empty(...)` before create/load to control the initial layout;
`new(...)` validates before a later builder call. Cancelled calls still consume
the call budget. A candidate cancelled during layout is rolled back with its
previous scene and revision. Replacing controls does not reset policy or budgets.
Free-region queries use the same control and checkpoint each completed
rectangle subtraction in the Preflight phase. Scene validation and final
filtering/sorting remain non-interruptible internally. Argument parsing checks
cancellation before, during a zero-allocation 16 KiB chunk walk, and after
Serde parsing. The bounded Serde call itself remains non-interruptible and may
consume up to the configured 1 MiB argument ceiling before the post-check.
Result conversion and other diagnostics still have cancellation gaps;
callbacks/observers must return promptly and cannot be forcibly stopped.

Preview/export (including CSV) stream exporter bytes through an 8 KiB base64
scratch into the bounded result String, retaining at most two raw bytes between
writes instead of a complete raw artifact. Metadata, table IDs, JSON escaping
and loss reports consume the same result budget, rechecked over the complete
envelope. This does not bound exporter or codec scratch memory.

Typed inspect/explain, preflight, debug-mask and free-region results are
counted against `max_result_bytes` before conversion to a JSON Value. Oversized
results stop at the counting pass, avoiding the additional JSON tree. The final
result check remains active. This does not bound the already-built resolved
scene or diagnostic DTOs, nor replace artifact/base64 envelope accounting.
Page inspection is serialized from a borrowed scene view: exclusion/region
names and overflow IDs are traversed directly, so rejection does not first
clone those lists into `PageInspection`. Accepted output keeps the exact core
page-inspection JSON shape and necessarily owns the final JSON strings.

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
