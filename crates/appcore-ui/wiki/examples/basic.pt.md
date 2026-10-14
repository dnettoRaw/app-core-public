# Página nativa — uso mínimo

O exemplo Rust executável abre texto e um botão nativos validados, drena uma fila limitada de ativações e fecha dentro do lifetime fixo. O deployment possui arranque, pump e shutdown.

```sh
cargo run -p appcore-ui --features native-macos --example native_page
```

[Rust](../../examples/native_page.rs) · [Guide](../guide.pt.md)

Estes exemplos estão incluídos no pacote beta público para macOS. Renderers de outras plataformas são trabalho futuro.
