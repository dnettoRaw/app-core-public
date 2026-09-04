# Exemple de base

Commencez avec une session vide et bornée, puis consultez le schéma sans créer
de document :

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

L'appel consomme le budget de la policy mais ne peut modifier aucun document.
