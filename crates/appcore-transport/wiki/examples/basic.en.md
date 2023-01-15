# Minimal bounded HTTP request

[Português](basic.pt.md) | [Français](basic.fr.md) |
[Intermediate example](intermediate.en.md) | [Guide](../guide.en.md)

Build a validated target and request without opening a socket.

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

Targets accept only HTTP or HTTPS, normalize the joined path and validate the
authority. Requests require uppercase methods and reject control characters in
headers.
