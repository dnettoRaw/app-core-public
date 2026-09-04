# Exemple de base

Consultez le contrat et validez un template sans écrire d'artifact :

```bash
cargo run -p appcore-filemaker-cli -- schema --json
cargo run -p appcore-filemaker-cli -- check \
  crates/appcore-filemaker-cli/examples/basic.yml --json
```

Le template exécutable est un fichier `.yml` séparé. Les fields inconnus et
versions non prises en charge échouent ; `check` ne lie
pas de données externes et ne choisit aucun exporter.
