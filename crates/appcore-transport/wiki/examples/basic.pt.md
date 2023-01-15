# Requisicao HTTP limitada minima

[English](basic.en.md) | [Français](basic.fr.md) |
[Exemplo intermediario](intermediate.pt.md) | [Guia](../guide.pt.md)

Construa um destino e uma requisicao validados sem abrir um socket.

```rust
use appcore_transport::{HttpHeader, HttpRequest, HttpTarget, TransportResult};

fn main() -> TransportResult<()> {
    let target = HttpTarget::parse("https://api.example.com", "/v1/health")?;
    let request = HttpRequest::new("GET", Vec::new())?
        .with_header(HttpHeader::new("Accept", "application/json")?);

    println!("{} {}", request.method(), target.path());
    assert_eq!(target.authority(), "api.example.com");
    Ok(())
}
```

Destinos aceitam apenas HTTP ou HTTPS, normalizam o path e validam a authority.
Requisicoes exigem metodos em maiusculas e rejeitam caracteres de controle nos
headers.
