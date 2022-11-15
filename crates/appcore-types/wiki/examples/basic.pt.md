# Contexto minimo de trace

[English](basic.en.md) | [Français](basic.fr.md) |
[Exemplo intermediario](intermediate.pt.md) | [Guia](../guide.pt.md)

Crie um trace raiz validado e derive um span filho para outro Core.

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

Identificadores sao validados na construcao. Spans filhos preservam tenant,
Core de origem, trace ID e a correlacao opcional do comando.
