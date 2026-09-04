# Exemplo intermediário

Compile e vincule um documento com `appcore-filemaker`, crie uma sessão com
policy pequena e explícita e ofereça `tool_definitions()` ao `appcore-ai`.
Execute chamadas retornadas por `execute_call`; nunca despache um nome não
declarado.

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

Mantenha loops do modelo limitados com `recommended_tool_loop()` e preserve
cada falha tipada do bridge em vez de repetir indefinidamente.
