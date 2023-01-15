# Cliente de health limitado

[English](intermediate.en.md) | [Français](intermediate.fr.md) |
[Exemplo minimo](basic.pt.md) | [Guia](../guide.pt.md)

Execute uma requisicao bloqueante de health com deadlines, limites de body,
redacao de header sensivel e cancelamento cooperativo explicitos.

```rust
use appcore_transport::{
    send, CancellationToken, HttpClientConfig, HttpHeader, HttpRequest,
    HttpTarget, TransportError, TransportResult,
};
use std::env;

fn fetch_health() -> TransportResult<Vec<u8>> {
    let base_url = env::var("SERVICE_URL")
        .map_err(|_| TransportError::InvalidRequest("SERVICE_URL is required".into()))?;
    let token = env::var("SERVICE_TOKEN")
        .map_err(|_| TransportError::InvalidRequest("SERVICE_TOKEN is required".into()))?;
    let target = HttpTarget::parse(&base_url, "/v1/health")?;
    let request = HttpRequest::new("GET", Vec::new())?
        .with_header(HttpHeader::new("Accept", "application/json")?)
        .with_header(HttpHeader::sensitive("Authorization", format!("Bearer {token}"))?);
    let cancellation = CancellationToken::new();
    let response = send(
        &target,
        &request,
        HttpClientConfig {
            timeout_ms: 2_000,
            max_request_bytes: 0,
            max_response_bytes: 64 * 1024,
            max_header_bytes: 16 * 1024,
        },
        Some(&cancellation),
    )?;
    if response.status_code != 200 {
        return Err(TransportError::InvalidResponse(format!(
            "health endpoint returned {}",
            response.status_code
        )));
    }
    Ok(response.body)
}
```

Mantenha autenticacao, retries e politica de status no adapter chamador. Este
crate fornece mecanica e limites de transporte; ele nao infere semantica da
aplicacao.
