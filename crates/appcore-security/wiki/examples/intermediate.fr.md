# Emettre un token lie a la requete

[English](intermediate.en.md) | [Português](intermediate.pt.md) |
[Exemple minimal](basic.fr.md) | [Guide](../guide.fr.md)

Liez un token de commande de courte duree au contenu exact de la requete. Le
secret de signature vient de l'environnement et n'est jamais affiche.

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

Persistez et refusez les valeurs `jti` reutilisees a la frontiere d'entree
authentifiee. HashToken signe les tokens; TLS et le stockage protege restent
des exigences du deploiement.
