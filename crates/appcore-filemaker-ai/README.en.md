# appcore-filemaker-ai

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

**PUBLIC BETA — `0.1.0-beta.2`.** APIs and behavior may change before stable
release. Validate outputs, limits and failure handling for your workload;
implementation and local tests are not production certification.

[Português](README.pt.md) | [Français](README.fr.md)

Optional bounded bridge between `appcore-ai` and `appcore-filemaker`. It keeps
model policy, tool schemas, call budgets, mutation validation, and artifact
access outside the deterministic FileMaker core.

All tool arguments use closed schemas, mutations resolve a candidate before
commit, and bridge limits can only tighten the core `ResourceLimits`.
Serialized result sizing writes into a non-retaining bounded counter and stops
at `max_result_bytes`; it does not allocate a second complete JSON buffer.

The complete create/patch/inspect/validate/preview/debug-mask/export loop is
executable and policy checked. Dataset sessions can export one selected table
as bounded in-memory CSV; graphical tools still require a resolved scene.
Capability discovery and export expose editable, flattened, and hybrid PDF;
hybrid combines vector outlines with invisible searchable Unicode text.
Schema discovery exposes `horizontal` and implemented `vertical_rl` writing;
only color emoji remains a prepared text capability.

See the [English guide](wiki/guide.en.md), [basic example](wiki/examples/basic.en.md),
and [intermediate example](wiki/examples/intermediate.en.md).

License: MIT.

## Stable documentation

Stable ID: **ACR-024**. See the
[supplemental architecture and integration guide](https://wiki.appcore.dnettoraw.com/crates/id/acr-024). This permanent ID
remains valid if the wiki page moves.
