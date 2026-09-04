# Intermediate example

Bind explicit data, preflight strictly, then render atomically:

```bash
cargo run -p appcore-filemaker-cli -- preflight \
  crates/appcore-filemaker-cli/examples/intermediate.yml \
  --data crates/appcore-filemaker-cli/examples/intermediate-data.json \
  --format pdf --strict --json
cargo run -p appcore-filemaker-cli -- render \
  crates/appcore-filemaker-cli/examples/intermediate.yml \
  --data crates/appcore-filemaker-cli/examples/intermediate-data.json \
  --format pdf --pdf-mode editable --output target/filemaker-example.pdf --json
```

The YAML and JSON inputs are separate runnable files. Keep the same inputs
between preflight and render. Strict mode rejects warnings;
the output file is published only after successful export. The CLI rejects an
output path that resolves to its input `.yml`, so this workflow cannot replace
its source template accidentally.
