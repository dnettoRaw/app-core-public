# Contrato minimo de query

[English](basic.en.md) | [Français](basic.fr.md) |
[Exemplo intermediario](intermediate.pt.md) | [Guia](../guide.pt.md)

Valide uma query estruturada e limitada e monte sua resposta controlada.

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

Handlers de query devem permanecer sem efeitos colaterais. Aplique autenticacao
e limite de payload antes do dispatch.
