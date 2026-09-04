# Basic example

[Português](basic.pt.md) | [Français](basic.fr.md) | [Intermediate](intermediate.en.md)

Run `cargo run -p appcore-filemaker --example basic`. It creates a complete
one-page A4 operations snapshot with bound title and owner text, semantic vector
drawings, a progress indicator, a cubic sparkline, and a first-class table with
conditional row styling and a checked numeric total. The SVG is written to
`target/filemaker-examples/basic.svg`.

The document is kept separately in
[`examples/basic.yml`](../../examples/basic.yml), and its typed input is
[`examples/basic-data.json`](../../examples/basic-data.json); the Rust runner
does not embed either payload in its source. It explicitly registers the
bundled OFL Noto Sans asset before layout, so no host font, filesystem asset,
network, or AI dependency is implicit. See
[`examples/basic.rs`](../../examples/basic.rs). The public order is:
`Compiler`, compile once, bind data and patches, `LayoutEngine`, then
`ExportRequest`.
