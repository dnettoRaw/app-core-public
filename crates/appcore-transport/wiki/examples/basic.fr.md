# Requete HTTP bornee minimale

[English](basic.en.md) | [Português](basic.pt.md) |
[Exemple intermediaire](intermediate.fr.md) | [Guide](../guide.fr.md)

Construisez une cible et une requete validees sans ouvrir de socket.

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

Les cibles n'acceptent que HTTP ou HTTPS, normalisent le chemin et valident
l'authority. Les requetes exigent des methodes en majuscules et refusent les
caracteres de controle dans les headers.
