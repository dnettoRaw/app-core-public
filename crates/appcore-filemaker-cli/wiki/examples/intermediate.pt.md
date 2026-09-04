# Exemplo intermediário

Vincule dados explícitos, execute preflight estrito e então renderize
atomicamente:

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

O YAML e o JSON são arquivos executáveis separados. Mantenha os mesmos inputs
entre preflight e render. O modo strict rejeita
warnings; o arquivo final só aparece depois do export bem-sucedido. A CLI
rejeita um output que resolve para seu `.yml` de entrada, portanto esse fluxo não
substitui seu template de origem por acidente.
