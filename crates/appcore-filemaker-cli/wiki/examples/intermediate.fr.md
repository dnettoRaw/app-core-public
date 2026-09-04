# Exemple intermédiaire

Liez les données explicites, exécutez un preflight strict, puis rendez
atomiquement :

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

Les entrées YAML et JSON sont des fichiers exécutables séparés. Gardez les
mêmes entrées entre preflight et render. Le mode strict refuse les
warnings ; le fichier final n'apparaît qu'après un export réussi. La CLI rejette
une sortie qui se résout vers son `.yml` d'entrée, donc ce flux ne remplace pas son
template source par accident.
