# Minimal three-artifact application

[Português](basic.pt.md) | [Français](basic.fr.md) |
[Intermediate example](intermediate.en.md) | [Guide](../guide.en.md)

This is the smallest standalone application: one Application Manifest, one
Deployment Manifest and business code. The Runtime owns all infrastructure.

`Cargo.toml`:

```toml
[package]
name = "notes-app"
version = "0.1.0"
edition = "2021"

[dependencies]
appcore-bin = "=1.0.0"
```

`application.toml`:

```toml
manifest_version = 1
application_id = "notes-app"
application_version = "0.1.0"
display_name = "Notes"
vendor = "Example"
service_id = "notes.runtime"
capabilities = []
leadership = []
dependencies = []
modules = []
feature_flags = {}
metadata = {}

[runtime]
minimum_runtime_version = "1.0.0"
protocol_version = "1"
required_features = []

[jobs]
enabled = false
max_concurrency = 0
retry_limit = 0

[storage]
durability = "local"
minimum_bytes = 0
shared = false

[scheduler]
required = false
max_concurrency = 0

[health]
startup_grace_ms = 30000
heartbeat_interval_ms = 10000
failure_threshold = 3

[update]
channel = "stable"
automatic = false
```

`deployment.toml`:

```toml
manifest_version = 1
installation_id = "notes-local"
application_id = "notes-app"
mode = "standalone"
secrets = { runtime_security = "env:APPCORE_RUNTIME_SECRET" }
paths = { storage = "data/storage", backup = "data/backups" }
volumes = []
adapters = {}
environment = {}

[storage]
provider_id = "file"
settings = {}
secret_refs = {}

[network]
listen_addresses = ["127.0.0.1:8080"]
peer_transport = "http"
command_transport = "http"

[network.tls]
enabled = false
```

`src/main.rs`:

```rust
use appcore_bin::application::{run_application, Application};

struct Notes;

impl Application for Notes {}

fn main() {
    if let Err(error) = run_application(&Notes) {
        eprintln!("application failed: {error}");
        std::process::exit(1);
    }
}
```

Populate `APPCORE_RUNTIME_SECRET` with high-entropy material from the deployment
secret manager, then run `cargo run`. The default paths are
`application.toml` and `deployment.toml` in the working directory.
