# Client de health borne

[English](intermediate.en.md) | [Português](intermediate.pt.md) |
[Exemple minimal](basic.fr.md) | [Guide](../guide.fr.md)

Exécutez des requêtes bloquantes de health avec un client réutilisable et
borné, des délais indépendants, des limites de body, le masquage des headers
sensibles et l'annulation coopérative.

```rust
use appcore_transport::{
    CancellationToken, HttpClient, HttpExchangeConfig, HttpHeader, HttpRequest,
    HttpTarget, HttpTimeouts, TransportError, TransportResult,
};
use std::env;

fn fetch_health(client: &HttpClient) -> TransportResult<Vec<u8>> {
    let base_url = env::var("SERVICE_URL")
        .map_err(|_| TransportError::InvalidRequest("SERVICE_URL is required".into()))?;
    let token = env::var("SERVICE_TOKEN")
        .map_err(|_| TransportError::InvalidRequest("SERVICE_TOKEN is required".into()))?;
    let target = HttpTarget::parse(&base_url, "/v1/health")?;
    let request = HttpRequest::new("GET", Vec::new())?
        .with_header(HttpHeader::new("Accept", "application/json")?)
        .with_header(HttpHeader::sensitive("Authorization", format!("Bearer {token}"))?);
    let cancellation = CancellationToken::new();
    let response = client.send(
        &target,
        &request,
        HttpExchangeConfig {
            timeouts: HttpTimeouts {
                connect_ms: 1_000,
                read_ms: 2_000,
                write_ms: 1_000,
            },
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

Créez un `HttpClient::default()` dans l'adaptateur propriétaire et transmettez-
le à chaque appel. Ses clones partagent le même pool borné par origine.
Conservez l'authentification, les retries et la politique de status dans cet
adaptateur ; ce crate ne déduit aucune sémantique applicative.
