# Exemplo básico

Comece com uma sessão vazia e limitada e consulte o schema sem criar um
documento:

```rust
let mut session = appcore_filemaker_ai::FileMakerAiSession::empty(
    appcore_filemaker::ResourceLimits::default(),
    appcore_filemaker::FontManager::default(),
    None,
    appcore_filemaker_ai::AiBridgePolicy::default(),
)?;
let schema = session.execute("filemaker_schema", "{}")?;
assert_eq!(schema.revision, 0);
# Ok::<(), appcore_filemaker_ai::BridgeError>(())
```

A chamada consome o budget da policy, mas não pode alterar um documento.
