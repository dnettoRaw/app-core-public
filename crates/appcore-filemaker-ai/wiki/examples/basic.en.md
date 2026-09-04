# Basic example

Start with an empty bounded session and inspect the schema without creating a
document:

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

The call consumes policy budget but cannot mutate a document.
