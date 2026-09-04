# Exemplo básico

Consulte o contrato e valide um template sem escrever artifact:

```bash
cargo run -p appcore-filemaker-cli -- schema --json
cargo run -p appcore-filemaker-cli -- check \
  crates/appcore-filemaker-cli/examples/basic.yml --json
```

O template executável é um arquivo `.yml` separado. Fields desconhecidos e
versões não suportadas falham; `check` não vincula dados
externos nem escolhe exporter.
