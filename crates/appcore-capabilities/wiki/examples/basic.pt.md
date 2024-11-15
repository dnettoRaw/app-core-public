# Capability local minima

[English](basic.en.md) | [Français](basic.fr.md) |
[Exemplo intermediario](intermediate.pt.md) | [Guia](../guide.pt.md)

Registre e invoque uma capability de query saudavel e local ao processo.

```rust
use appcore_capabilities::{
    CapabilityRegistry, CapabilityRequest, CapabilityResponse, CapabilityResult,
    LocalCapabilityHandler,
};
use appcore_core::{
    CapabilityDescriptor, CapabilityMode, CapabilityName, CapabilityVisibility,
};

struct VersionQuery {
    descriptor: CapabilityDescriptor,
}

impl LocalCapabilityHandler for VersionQuery {
    fn descriptor(&self) -> CapabilityDescriptor { self.descriptor.clone() }

    fn handle(&self, _request: &CapabilityRequest) -> CapabilityResult<CapabilityResponse> {
        Ok(CapabilityResponse::accepted(b"1.4.0".to_vec(), None))
    }
}

fn main() -> Result<(), String> {
    let name = CapabilityName::new("application.version")
        .map_err(|error| format!("{error:?}"))?;
    let mut registry = CapabilityRegistry::new();
    registry
        .register_handler(VersionQuery {
            descriptor: CapabilityDescriptor {
                name: name.clone(),
                version: "1".to_string(),
                mode: CapabilityMode::Query,
                visibility: CapabilityVisibility::Local,
                requirements: appcore_capabilities::requirements_for_read_only(),
            },
        })
        .map_err(|error| format!("{error:?}"))?;
    let response = registry
        .get(&name)
        .ok_or_else(|| "provider missing".to_string())?
        .handle(&CapabilityRequest {
            request_id: "request-42".to_string(),
            capability: name,
            mode: CapabilityMode::Query,
            payload: Vec::new(),
            idempotency_key: None,
            trace: None,
        })
        .map_err(|error| format!("{error:?}"))?;

    println!("accepted={} bytes={}", response.accepted, response.payload.len());
    Ok(())
}
```

O registry rejeita nomes duplicados e nunca seleciona um provider remoto.
