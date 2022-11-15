# Contexte de trace minimal

[English](basic.en.md) | [Português](basic.pt.md) |
[Exemple intermediaire](intermediate.fr.md) | [Guide](../guide.fr.md)

Creez une trace racine validee puis un span enfant pour un autre Core.

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

Les identifiants sont valides a la construction. Les spans enfants conservent
le tenant, le Core d'origine, le trace ID et la correlation de commande.
