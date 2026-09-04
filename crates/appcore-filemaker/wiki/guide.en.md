# appcore-filemaker guide

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
