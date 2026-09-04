# Exemple intermédiaire

Compilez et liez un document avec `appcore-filemaker`, créez une session avec
une policy petite et explicite, puis fournissez `tool_definitions()` à
`appcore-ai`. Exécutez les appels reçus via `execute_call` ; ne distribuez jamais
un nom non déclaré.

```rust
let policy = appcore_filemaker_ai::AiBridgePolicy {
    max_tool_calls: 8,
    max_argument_bytes: 16 * 1024,
    max_patch_operations: 4,
    max_result_bytes: 64 * 1024,
    allow_document_replacement: false,
    ..Default::default()
};
let mut session = appcore_filemaker_ai::FileMakerAiSession::new(
    document,
    limits,
    fonts,
    None,
    policy,
)?;
let result = session.execute("filemaker_validate", "{}")?;
assert_eq!(result.revision, 0);
# Ok::<(), appcore_filemaker_ai::BridgeError>(())
```

Bornez les boucles du modèle avec `recommended_tool_loop()` et conservez chaque
échec typé du bridge au lieu de réessayer indéfiniment.
