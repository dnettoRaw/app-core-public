# appcore-filemaker-cli

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
