# Contrat de query minimal

[English](basic.en.md) | [Português](basic.pt.md) |
[Exemple intermediaire](intermediate.fr.md) | [Guide](../guide.fr.md)

Validez une query structuree et bornee, puis construisez sa reponse controlee.

```rust
use appcore_api::{QueryRequest, QueryResponse};
use serde_json::json;

fn main() -> Result<(), String> {
    let request = QueryRequest {
        query_name: "document.get".to_string(),
        query_id: "query-42".to_string(),
        payload: json!({ "document_id": "42" }),
    };
    request
        .validate(16 * 1024)
        .map_err(|error| format!("{error:?}"))?;
    let response = QueryResponse::ok(json!({
        "document_id": "42",
        "revision": 7
    }));

    println!("query={} ok={}", request.query_name, response.ok);
    Ok(())
}
```

Les handlers de query doivent rester sans effet de bord. Appliquez
l'authentification et une limite de payload avant le dispatch.
