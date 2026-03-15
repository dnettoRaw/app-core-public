// =============================================================================
//        #######
//     ###       ###     F: auth_server_grant.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/07 08:56:37 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/21 10:48:21 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! HashToken grants issued by the optional auth-server companion.

use appcore_security::{
    parse_secret_material, HashTokenProvider, SecuritySecretStatus, TokenClaims, TokenProvider,
};
use std::fs;
use std::path::Path;

pub const DEFAULT_AUTH_GRANT_TTL_MS: u64 = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthGrant {
    pub resource: String,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
}

pub fn issue_auth_grant(
    transport_secret_path: &Path,
    resource: &str,
    ttl_ms: u64,
    now_ms: u64,
) -> Result<String, String> {
    validate_resource(resource)?;
    let ttl_ms = validate_ttl(ttl_ms)?;
    let provider = load_provider(transport_secret_path, now_ms)?;
    let grant = AuthGrant {
        resource: resource.to_string(),
        issued_at_ms: now_ms,
        expires_at_ms: now_ms.saturating_add(ttl_ms),
    };
    let token = provider
        .seal(grant_payload(&grant).as_bytes(), &grant_claims(ttl_ms))
        .map_err(|_| "auth grant generation failed".to_string())?;
    String::from_utf8(token).map_err(|_| "auth grant is not valid UTF-8".to_string())
}

pub fn open_auth_grant(secret_path: &Path, token: &str, now_ms: u64) -> Result<AuthGrant, String> {
    let provider = load_provider(secret_path, now_ms)?;
    let payload = provider
        .open(
            token.as_bytes(),
            &grant_claims(DEFAULT_AUTH_GRANT_TTL_MS.max(1)),
        )
        .map_err(|_| "auth grant verification failed".to_string())?;
    let grant = parse_grant_payload(&payload)?;
    if grant.expires_at_ms <= now_ms {
        return Err("auth grant expired".to_string());
    }
    Ok(grant)
}

fn load_provider(secret_path: &Path, now_ms: u64) -> Result<HashTokenProvider, String> {
    let bytes =
        fs::read(secret_path).map_err(|_| "auth transport secret file missing".to_string())?;
    let material = parse_secret_material(&bytes)
        .map_err(|_| "auth transport secret file invalid".to_string())?;
    reject_unusable_secret(
        material.metadata.status.clone(),
        material.is_expired(now_ms),
    )?;
    HashTokenProvider::from_secret(material.secret.clone())
        .map_err(|_| "auth transport secret too weak".to_string())
}

fn reject_unusable_secret(status: SecuritySecretStatus, expired: bool) -> Result<(), String> {
    if status == SecuritySecretStatus::Revoked {
        return Err("auth transport secret revoked".to_string());
    }
    if expired {
        return Err("auth transport secret expired".to_string());
    }
    Ok(())
}

fn validate_resource(resource: &str) -> Result<(), String> {
    if resource.is_empty() || resource.len() > 256 || resource.contains('\n') {
        return Err("invalid auth grant resource".to_string());
    }
    Ok(())
}

fn validate_ttl(ttl_ms: u64) -> Result<u64, String> {
    if ttl_ms == 0 || ttl_ms > 60_000 {
        return Err("auth grant ttl must be between 1 and 60000 ms".to_string());
    }
    Ok(ttl_ms)
}

fn grant_claims(ttl_ms: u64) -> TokenClaims {
    TokenClaims {
        issuer: "appcore-auth-server".to_string(),
        audience: "appcore-runtime-host".to_string(),
        salt: "auth-grant-v1".to_string(),
        ttl_ms,
    }
}

fn grant_payload(grant: &AuthGrant) -> String {
    format!(
        "schema=appcore.auth-grant.v1\nresource={}\nissued_at_ms={}\nexpires_at_ms={}\n",
        grant.resource, grant.issued_at_ms, grant.expires_at_ms
    )
}

fn parse_grant_payload(bytes: &[u8]) -> Result<AuthGrant, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "auth grant payload invalid".to_string())?;
    let resource = value_for(text, "resource")?;
    let issued_at_ms = parse_u64_value(text, "issued_at_ms")?;
    let expires_at_ms = parse_u64_value(text, "expires_at_ms")?;
    Ok(AuthGrant {
        resource,
        issued_at_ms,
        expires_at_ms,
    })
}

fn parse_u64_value(text: &str, key: &str) -> Result<u64, String> {
    value_for(text, key)?
        .parse::<u64>()
        .map_err(|_| format!("auth grant {key} invalid"))
}

fn value_for(text: &str, key: &str) -> Result<String, String> {
    for line in text.lines() {
        if let Some(value) = line.strip_prefix(&format!("{key}=")) {
            return Ok(value.to_string());
        }
    }
    Err(format!("auth grant {key} missing"))
}

#[cfg(test)]
#[path = "auth_server_grant_tests.rs"]
mod tests;
