# Issue a request-bound command token

[Português](intermediate.pt.md) | [Français](intermediate.fr.md) |
[Minimal example](basic.en.md) | [Guide](../guide.en.md)

Bind a short-lived command token to the exact request contents. The signing
secret comes from the deployment environment and is never printed.

```rust
use appcore_security::{
    compute_request_hash, CommandTokenFactory, CommandTokenValidator,
    HashTokenProvider, RequestValidationDetails, TokenClaims,
};

fn main() -> Result<(), String> {
    let secret = std::env::var("APPCORE_TOKEN_SECRET")
        .map_err(|error| error.to_string())?
        .into_bytes();
    let provider = HashTokenProvider::from_secret(secret)
        .map_err(|error| format!("provider: {error:?}"))?;
    let trust = TokenClaims {
        issuer: "notes-runtime".to_string(),
        audience: "notes-api".to_string(),
        salt: "command-v1".to_string(),
        ttl_ms: 30_000,
    };
    let request = RequestValidationDetails {
        purpose: "command".to_string(),
        name: "document.create".to_string(),
        id: "command-42".to_string(),
        idempotency_key: Some("create-document-42".to_string()),
        payload: r#"{"title":"Runtime notes"}"#.to_string(),
        subject: Some("worker-7".to_string()),
        audience: Some("notes-api".to_string()),
    };
    let request_hash = compute_request_hash(&request);
    let token = CommandTokenFactory::new(&provider, trust.clone())
        .create_v1_with_jti_and_hash(
            "command",
            Some("document.create"),
            None,
            Some("worker-7"),
            1_700_000_000_000,
            30_000,
            Some("token-command-42".to_string()),
            Some(request_hash.clone()),
        )
        .map_err(|error| format!("issue: {error:?}"))?;
    let claims = CommandTokenValidator::new(&provider, trust)
        .validate_and_get_claims(
            &token,
            "command",
            Some("document.create"),
            1_700_000_010_000,
            Some(&request_hash),
        )
        .map_err(|error| format!("validate: {error:?}"))?;

    println!("subject={:?} expires={}", claims.subject, claims.expires_at_ms);
    Ok(())
}
```

Persist and reject reused `jti` values at the authenticated ingress boundary.
HashToken signs tokens; TLS and protected secret storage remain deployment
requirements.

## Operate the Windows DPAPI keyring

In the `1.0.2-rc` Windows build, create and rotate an explicitly selected
current-user keyring without supplying secret bytes on the command line:

```powershell
appcore-bin security secret keyring-init --keyring C:\AppCore\security --keyring-provider windows-dpapi-user-v1
appcore-bin security secret keyring-rotate --keyring C:\AppCore\security --keyring-provider windows-dpapi-user-v1
appcore-bin security secret keyring-status --keyring C:\AppCore\security --keyring-provider windows-dpapi-user-v1
```

Run every command as the same deployment identity. Copy the complete directory
only for same-user/same-machine restore; another identity or machine must fail
closed. Keep the previous provider directory until the restored deployment
passes its health gate.
