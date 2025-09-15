# Minimal query contract

[Português](basic.pt.md) | [Français](basic.fr.md) |
[Intermediate example](intermediate.en.md) | [Guide](../guide.en.md)

Validate one bounded structured query and build its controlled response.

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

Query handlers must remain side-effect free. Apply authentication and a payload
bound before dispatch.
