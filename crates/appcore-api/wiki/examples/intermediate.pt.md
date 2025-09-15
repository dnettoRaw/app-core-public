# Rotear uma query da aplicacao

[English](intermediate.en.md) | [Français](intermediate.fr.md) |
[Exemplo minimo](basic.pt.md) | [Guia](../guide.pt.md)

Registre um endpoint independente de transporte e despache uma requisicao pelo
router deterministico da API.

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

O router cuida apenas do dispatch. Autenticacao HTTP, decode da requisicao e
encode da resposta ficam no limite de transporte.
