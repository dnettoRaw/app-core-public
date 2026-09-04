# Exemple de base

[English](basic.en.md) | [Português](basic.pt.md) | [Intermédiaire](intermediate.fr.md)

Exécutez `cargo run -p appcore-filemaker --example basic`. Il crée un rapport
opérationnel A4 complet d'une page avec titre et responsable liés aux données,
dessins vectoriels sémantiques, indicateur de progression, sparkline cubique et
table de première classe avec style conditionnel et total numérique vérifié. Le
SVG est écrit dans `target/filemaker-examples/basic.svg`.

Le document reste séparé dans
[`examples/basic.yml`](../../examples/basic.yml), et les données typées dans
[`examples/basic-data.json`](../../examples/basic-data.json) ; le lanceur Rust
n'intègre aucun payload dans son code source. Il enregistre explicitement la
police Noto Sans sous OFL fournie avant le layout : police hôte, asset fichier,
réseau et IA ne sont jamais implicites. Voir
[`examples/basic.rs`](../../examples/basic.rs). L'ordre public est :
`Compiler`, compilation unique, données et patches, `LayoutEngine`, puis
`ExportRequest`.
