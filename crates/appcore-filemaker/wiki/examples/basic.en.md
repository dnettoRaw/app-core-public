# Basic example

[Português](basic.pt.md) | [Français](basic.fr.md) | [Intermediate](intermediate.en.md)

Run `cargo run -p appcore-filemaker --example basic`. It creates a complete
one-page A4 operations snapshot with bound title and owner text, semantic vector
drawings, a progress indicator, a cubic sparkline, and a first-class table with
conditional row styling and a checked numeric total. The SVG is written to
`target/filemaker-examples/basic.svg`.
The bound subtitle also demonstrates generic per-side text-block padding; its
insets affect measurement and all visual exporters, not application data.

The document is kept separately in
[`examples/basic.yml`](../../examples/basic.yml), and its typed input is
[`examples/basic-data.json`](../../examples/basic-data.json); the Rust runner
does not embed either payload in its source. It explicitly registers the
bundled OFL Noto Sans asset before layout, so no host font, filesystem asset,
network, or AI dependency is implicit. See
[`examples/basic.rs`](../../examples/basic.rs). The public order is:
`Compiler`, compile once, bind data and patches, `LayoutEngine`, then
`ExportRequest`.

## Native symbols (unpublished source)

Keep symbol geometry and payload bindings in YAML. See the guide for checksums, quiet zones and budgets.

```yaml
- id: code
  type: barcode
  x: 10pt
  y: 10pt
  width: 162pt
  height: 40pt
  text: '96385074'
- id: qr
  type: qr
  x: 10pt
  y: 70pt
  width: 140pt
  height: 140pt
  text: 'https://example.org/local'
```

Additional bounded contract example; execute in a Result-returning context with explicitly selected dependencies.

```rust
use appcore_filemaker::{FileMakerError, ErrorCode};
let error = FileMakerError::new(ErrorCode::LayoutInvalid, "text fitting failed").at("elements.title.text");
let diagnostic = error.diagnostic_v1();
assert_eq!(diagnostic.code, "FM-LAYOUT-INVALID");
assert_eq!(diagnostic.source_path.as_deref(), Some("elements.title.text"));
```
