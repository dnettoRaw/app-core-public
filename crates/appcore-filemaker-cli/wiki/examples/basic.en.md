# Basic example

Inspect the contract and validate a template without writing an artifact:

```bash
cargo run -p appcore-filemaker-cli -- schema --json
cargo run -p appcore-filemaker-cli -- check \
  crates/appcore-filemaker-cli/examples/basic.yml --json
```

The runnable template is a separate `.yml` file. Unknown fields and unsupported
schema versions fail; `check` does not bind
external data or choose an exporter.
