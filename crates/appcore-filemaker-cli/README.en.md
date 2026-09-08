# appcore-filemaker-cli

Render, CSV and collision-mask outputs stream through a 64 KiB buffer into
an exclusive temporary file. The CLI no longer retains the complete encoded
output before atomic publication. Export/flush failures preserve an existing
destination and attempt to remove staging. Exporter-internal scratch memory remains
subject to the core limits; this is not a zero-allocation export guarantee.

**PUBLIC BETA — `0.1.0-beta.2`.** APIs and behavior may change before stable
release. Validate outputs, limits and failure handling for your workload;
implementation and local tests are not production certification.

[Português](README.pt.md) | [Français](README.fr.md)

Bounded command-line adapter for `appcore-filemaker`. It provides schema,
validation, preflight, inspection, debug, mask, and atomic render commands with
stable JSON output and typed exit codes.
Human and pretty-JSON stdout are sized under a 512 MiB cap, then written through
fixed buffers without retaining a second complete output `String`.

The CLI applies repeatable JSON runtime patches, configures an ordered explicit
font fallback, queries free regions, and exports bounded table datasets as CSV
without routing dataset rows through graphical layout.
`render --format pdf --pdf-mode hybrid` writes deterministic outlines plus an
invisible subsetted Unicode layer for searchable, selectable output.
`schema --json` reports `horizontal` and `vertical_rl` as implemented writing
modes; only color emoji remains a prepared text capability.

Runnable YAML documents and data are separate files under `examples/`; command
examples do not hide templates inside Rust or shell source.

See the [English guide](wiki/guide.en.md), [basic example](wiki/examples/basic.en.md),
and [intermediate example](wiki/examples/intermediate.en.md).

License: MIT.

## Stable documentation

Stable ID: **ACR-025**. See the
[supplemental architecture and integration guide](https://wiki.appcore.dnettoraw.com/crates/id/acr-025). This permanent ID
remains valid if the wiki page moves.
