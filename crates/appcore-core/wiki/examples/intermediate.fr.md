# Dispatcher une commande applicative idempotente

[English](intermediate.en.md) | [Português](intermediate.pt.md) |
[Exemple minimal](basic.fr.md) | [Guide](../guide.fr.md)

A la frontiere bas niveau, enregistrez un handler, dispatchez une commande
validee et retournez un evenement de fait. Les nouvelles applications font
normalement cette composition via la facade `Application` de `appcore-bin`.

```rust
use appcore_core::{
    AppFamily, AppId, CommandBus, CommandEnvelope, CommandHandler, CommandName,
    CommandResult, EventEnvelope, EventName, NodeId, RuntimeContext,
    RuntimeContractVersion, RuntimeResult, SyncGroup,
};

struct Context {
    app_id: AppId,
    family: AppFamily,
    sync_group: SyncGroup,
    node_id: NodeId,
}

impl RuntimeContext for Context {
    fn app_id(&self) -> &AppId { &self.app_id }
    fn app_family(&self) -> &AppFamily { &self.family }
    fn sync_group(&self) -> &SyncGroup { &self.sync_group }
    fn runtime_contract(&self) -> RuntimeContractVersion {
        RuntimeContractVersion::new(1)
    }
    fn node_id(&self) -> &NodeId { &self.node_id }
}

struct CreateDocument {
    name: CommandName,
}

impl CommandHandler for CreateDocument {
    fn command_name(&self) -> CommandName {
        self.name.clone()
    }

    fn handle(
        &self,
        command: &CommandEnvelope,
        context: &dyn RuntimeContext,
    ) -> RuntimeResult<CommandResult> {
        let event = EventEnvelope::new(
            EventName::new("document.created".to_string())?,
            format!("event-{}", command.command_id),
            context.app_id().clone(),
            context.node_id().clone(),
            command.issued_at_ms,
            command.payload().to_vec(),
        )?;
        Ok(CommandResult::accepted(vec![event]))
    }
}

fn main() -> RuntimeResult<()> {
    let context = Context {
        app_id: AppId::new("documents-app".to_string())?,
        family: AppFamily::new("documents".to_string())?,
        sync_group: SyncGroup::new("tenant-acme".to_string())?,
        node_id: NodeId::new("node-paris".to_string())?,
    };
    let command_name = CommandName::new("document.create".to_string())?;
    let mut bus = CommandBus::new();
    bus.register_handler(CreateDocument {
        name: command_name.clone(),
    })?;

    let command = CommandEnvelope::new(
        command_name,
        "command-42".to_string(),
        context.app_id.clone(),
        context.node_id.clone(),
        1_700_000_000_000,
        Some("create-document-42".to_string()),
        br#"{"title":"Runtime notes"}"#.to_vec(),
    )?;
    let result = bus.dispatch(&command, &context)?;

    println!("accepted={} events={}", result.is_accepted(), result.events().len());
    Ok(())
}
```

Le handler reste deterministe et retourne des faits. L'idempotence durable, la
publication des evenements et l'audit sont des responsabilites de composition
autour de ce contrat.
