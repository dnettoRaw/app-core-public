# Page native — utilisation minimale

L’exemple Rust exécutable ouvre du texte et un bouton natifs validés, traite une file bornée d’activations et ferme dans sa durée de vie fixe. Le deployment possède démarrage, pump et arrêt.

```sh
cargo run -p appcore-ui --features native-macos --example native_page
```

[Rust](../../examples/native_page.rs) · [Guide](../guide.fr.md)

Ces exemples sont inclus dans le paquet bêta public pour macOS. Les renderers des autres plateformes restent des travaux futurs.
