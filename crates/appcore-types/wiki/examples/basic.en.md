# Minimal trace context

[Português](basic.pt.md) | [Français](basic.fr.md) |
[Intermediate example](intermediate.en.md) | [Guide](../guide.en.md)

Create a validated root trace and derive a child span for another Core.

```rust
use appcore_types::{CoreId, RuntimeResult, TenantId, TraceContext};

fn main() -> RuntimeResult<()> {
    let origin = CoreId::new("core-api")?;
    let trace = TraceContext::new(
        "trace-42",
        "span-api",
        origin.clone(),
        origin,
        TenantId::new("tenant-a")?,
    )?
    .with_command_id("command-7")?;
    let child = trace.child_span("span-worker", CoreId::new("core-worker")?)?;

    println!("{} <- {:?}", child.span_id, child.parent_span_id);
    Ok(())
}
```

Identifiers are validated at construction. Child spans preserve tenant,
originating Core, trace ID and optional command correlation.
