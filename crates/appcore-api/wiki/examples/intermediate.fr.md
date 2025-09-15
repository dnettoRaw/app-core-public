# Router une query applicative

[English](intermediate.en.md) | [Português](intermediate.pt.md) |
[Exemple minimal](basic.fr.md) | [Guide](../guide.fr.md)

Enregistrez un endpoint independant du transport et dispatchez une requete via
le router deterministe de l'API.

```rust
use appcore_api::{
    ApiMethod, ApiRequest, ApiResponse, ApiRouter, QueryEndpoint, QueryName,
};
use appcore_core::{RuntimeError, RuntimeResult};

struct DocumentStatus {
    name: QueryName,
}

impl QueryEndpoint for DocumentStatus {
    fn query_name(&self) -> &QueryName { &self.name }

    fn handle_query(&self, request: ApiRequest) -> RuntimeResult<ApiResponse> {
        if request.method != ApiMethod::Query {
            return Err(RuntimeError::InvalidRequest {
                kind: "query",
                reason: "method mismatch",
            });
        }
        if request.payload.len() > 16 * 1024 {
            return Err(RuntimeError::InvalidRequest {
                kind: "query",
                reason: "payload too large",
            });
        }
        Ok(ApiResponse {
            status_code: 200,
            payload: br#"{"document_id":"42","status":"ready"}"#.to_vec(),
        })
    }
}

fn main() -> RuntimeResult<()> {
    let name = QueryName::new("document.status".to_string())?;
    let mut router = ApiRouter::new();
    router.register_query(DocumentStatus { name: name.clone() })?;
    let response = router.dispatch_query(
        &name,
        ApiRequest {
            method: ApiMethod::Query,
            path: "/v1/query/document.status".to_string(),
            payload: br#"{"document_id":"42"}"#.to_vec(),
        },
    )?;

    println!("status={} bytes={}", response.status_code, response.payload.len());
    Ok(())
}
```

Le router ne gere que le dispatch. L'authentification HTTP, le decodage de la
requete et l'encodage de la reponse restent a la frontiere du transport.
