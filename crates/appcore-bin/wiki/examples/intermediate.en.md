# Application with command, event and query

[Português](intermediate.pt.md) | [Français](intermediate.fr.md) |
[Minimal example](basic.en.md) | [Guide](../guide.en.md)

Start from the minimal example, add JSON dependencies, declare two capabilities
and register only application behavior. Runtime composition remains unchanged.

Add to `Cargo.toml`:

```toml
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

Add to `application.toml` in place of `capabilities = []`:

```toml
[[capabilities]]
id = "document.create"
version = "1"
mode = "command"
visibility = "local"
requires_leader = false
idempotency_required = true

[[capabilities]]
id = "document.status"
version = "1"
mode = "query"
visibility = "local"
requires_leader = false
idempotency_required = false
```

Replace `src/main.rs`:

```rust
use appcore_bin::application::{
    run_application, ApiRequest, ApiResponse, ApiRouter, Application, CommandBus,
    CommandEnvelope, CommandHandler, CommandName, CommandRegistry, CommandResult,
    EventEnvelope, EventName, EventRegistry, QueryEndpoint, QueryName,
    RuntimeContext, RuntimeResult,
};
use serde::Deserialize;

struct Notes {
    create: CommandName,
    created: EventName,
    status: QueryName,
}

impl Notes {
    fn new() -> RuntimeResult<Self> {
        Ok(Self {
            create: CommandName::new("document.create".to_string())?,
            created: EventName::new("document.created".to_string())?,
            status: QueryName::new("document.status".to_string())?,
        })
    }
}

impl Application for Notes {
    fn register_commands(&self, registry: &mut CommandRegistry) -> RuntimeResult<()> {
        registry.register(self.create.clone())
    }

    fn register_events(&self, registry: &mut EventRegistry) -> RuntimeResult<()> {
        registry.register(self.created.clone())
    }

    fn register_handlers(&self, bus: &mut CommandBus) -> RuntimeResult<()> {
        bus.register_handler(CreateDocument {
            command: self.create.clone(),
            event: self.created.clone(),
        })
    }

    fn register_queries(&self, router: &mut ApiRouter) -> RuntimeResult<()> {
        router.register_query(DocumentStatus {
            name: self.status.clone(),
        })
    }
}

struct CreateDocument {
    command: CommandName,
    event: EventName,
}

#[derive(Deserialize)]
struct CreateDocumentPayload {
    document_id: String,
    title: String,
}

impl CommandHandler for CreateDocument {
    fn command_name(&self) -> CommandName { self.command.clone() }

    fn handle(
        &self,
        command: &CommandEnvelope,
        _context: &dyn RuntimeContext,
    ) -> RuntimeResult<CommandResult> {
        if command.idempotency_key.is_none() {
            return Ok(CommandResult::rejected("idempotency key required"));
        }
        let payload = match serde_json::from_slice::<CreateDocumentPayload>(command.payload()) {
            Ok(payload) => payload,
            Err(_) => return Ok(CommandResult::rejected("invalid payload")),
        };
        if payload.document_id.is_empty() || payload.title.trim().is_empty() {
            return Ok(CommandResult::rejected("document_id and title are required"));
        }
        let event = EventEnvelope::new(
            self.event.clone(),
            format!("event-{}", command.command_id),
            command.app_id.clone(),
            command.node_id.clone(),
            command.issued_at_ms,
            command.payload().to_vec(),
        )?;
        Ok(CommandResult::accepted(vec![event]))
    }
}

struct DocumentStatus {
    name: QueryName,
}

impl QueryEndpoint for DocumentStatus {
    fn query_name(&self) -> &QueryName { &self.name }

    fn handle_query(&self, request: ApiRequest) -> RuntimeResult<ApiResponse> {
        if request.payload.len() > 16 * 1024 {
            return Ok(ApiResponse {
                status_code: 413,
                payload: Vec::new(),
            });
        }
        Ok(ApiResponse {
            status_code: 200,
            payload: br#"{"status":"ready"}"#.to_vec(),
        })
    }
}

fn run() -> Result<(), String> {
    let application = Notes::new().map_err(|error| format!("{error:?}"))?;
    run_application(&application).map_err(|error| error.to_string())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("application failed: {error}");
        std::process::exit(1);
    }
}
```

Mutable HTTP calls must provide an idempotency key. The handler validates its
application payload, returns typed control flow and emits a fact event; the
Runtime owns transport, audit, persistence and shutdown.
