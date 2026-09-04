# Intermediate example

Compile and bind a document with `appcore-filemaker`, create a session with a
small explicit policy, and offer `tool_definitions()` to `appcore-ai`. Execute
returned calls through `execute_call`; never dispatch an undeclared name.

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

Keep model loops bounded with `recommended_tool_loop()` and preserve every
typed bridge failure instead of retrying indefinitely.
