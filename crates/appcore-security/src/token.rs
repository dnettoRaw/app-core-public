// =============================================================================
//        #######
//     ###       ###     F: token.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/31 13:38:42 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Token contracts for internal runtime trust and delegation.
// ATENÇÃO: Isso não é segurança perfeita. Não protege contra invasão física ou comprometimento das chaves.
// A segurança real depende de chaves simétricas bem protegidas e tráfego encapsulado em TLS/mTLS.

pub use crate::request_hash::{compute_request_hash, RequestValidationDetails};
use serde::{Deserialize, Serialize};

/// Default lifetime for locally issued Runtime tokens.
pub const DEFAULT_RUNTIME_TOKEN_TTL_MS: u64 = 60_000;
/// Explicit subject required for wildcard local-administration scope.
pub const LOCAL_ADMIN_SUBJECT: &str = "local-admin";

/// Security-local result type.
pub type SecurityResult<T> = Result<T, SecurityError>;

/// Security-local errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecurityError {
    /// Requested security operation is unsupported.
    Unsupported(&'static str),
    /// Token structure or claims are invalid.
    InvalidToken,
    /// Signature or payload verification failed.
    VerificationFailed,
    /// Secret reference is unsafe or malformed.
    InvalidSecretRef,
    /// Referenced secret material is unavailable.
    SecretUnavailable,
}

/// Command-token specific validation/generation errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandTokenError {
    /// Bearer token structure or requested claim combination is invalid.
    InvalidFormat,
    /// Token is absent, expired, invalid or for another purpose.
    Unauthorized,
    /// Token is valid but does not permit the requested resource.
    Forbidden,
}

/// Minimal token claims contract for internal token exchange.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenClaims {
    /// Expected token issuer.
    pub issuer: String,
    /// Expected token audience.
    pub audience: String,
    /// Provider-specific non-secret salt label.
    pub salt: String,
    /// Token lifetime in milliseconds.
    pub ttl_ms: u64,
}

/// Bearer claims contract for `/command` authentication (v1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeTokenClaims {
    /// Claims schema version.
    pub version: String,
    /// Isolated token purpose such as command, query, sync or peer.
    pub purpose: String,
    /// Optional command or query name.
    pub command_name: Option<String>,
    /// Optional explicit scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Optional authenticated subject.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    /// Issue timestamp in Unix milliseconds.
    pub issued_at_ms: u64,
    /// Expiry timestamp in Unix milliseconds.
    pub expires_at_ms: u64,
    /// Optional single-use token identity persisted by the host replay store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jti: Option<String>,
    // Hash SHA-256 do payload. Protege contra adulteração em trânsito, mas exige determinismo exato de ambos os lados.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Optional digest binding the token to one exact request.
    pub request_hash: Option<String>,
}

/// Factory de tokens bearer do runtime.
pub struct CommandTokenFactory<'a, P: TokenProvider> {
    provider: &'a P,
    claims: TokenClaims,
}

/// Validador de tokens bearer do runtime.
pub struct CommandTokenValidator<'a, P: TokenProvider> {
    provider: &'a P,
    claims: TokenClaims,
}

/// Contrato de assinatura e validação de payloads.
pub trait TokenProvider {
    /// Authenticates and encrypts payload bytes.
    fn seal(&self, payload: &[u8], claims: &TokenClaims) -> SecurityResult<Vec<u8>>;
    /// Authenticates and decrypts token bytes.
    fn open(&self, token: &[u8], claims: &TokenClaims) -> SecurityResult<Vec<u8>>;
    /// Produces an authenticated signature carrying payload bytes.
    fn sign(&self, payload: &[u8], claims: &TokenClaims) -> SecurityResult<Vec<u8>>;
    /// Verifies that a signature carries the expected payload and claims.
    fn verify(&self, payload: &[u8], signature: &[u8], claims: &TokenClaims) -> SecurityResult<()>;
}

impl<'a, P: TokenProvider> CommandTokenFactory<'a, P> {
    /// Creates a bearer token factory.
    pub fn new(provider: &'a P, claims: TokenClaims) -> Self {
        Self { provider, claims }
    }

    /// Issues a V1 command token scoped to one command name.
    pub fn create_v1(
        &self,
        command_name: Option<&str>,
        subject: Option<&str>,
        issued_at_ms: u64,
        ttl_ms: u64,
    ) -> Result<String, CommandTokenError> {
        self.create_v1_scoped(command_name, None, subject, issued_at_ms, ttl_ms)
    }

    /// Issues a V1 command token with an explicit scope.
    pub fn create_v1_scoped(
        &self,
        command_name: Option<&str>,
        scope: Option<&str>,
        subject: Option<&str>,
        issued_at_ms: u64,
        ttl_ms: u64,
    ) -> Result<String, CommandTokenError> {
        self.create_v1_for_purpose_scoped(
            "command",
            command_name,
            scope,
            subject,
            issued_at_ms,
            ttl_ms,
        )
    }

    /// Issues a V1 token for an isolated purpose.
    pub fn create_v1_for_purpose(
        &self,
        purpose: &str,
        command_name: Option<&str>,
        subject: Option<&str>,
        issued_at_ms: u64,
        ttl_ms: u64,
    ) -> Result<String, CommandTokenError> {
        self.create_v1_for_purpose_scoped(
            purpose,
            command_name,
            None,
            subject,
            issued_at_ms,
            ttl_ms,
        )
    }

    /// Issues a V1 purpose token with an explicit scope.
    pub fn create_v1_for_purpose_scoped(
        &self,
        purpose: &str,
        command_name: Option<&str>,
        scope: Option<&str>,
        subject: Option<&str>,
        issued_at_ms: u64,
        ttl_ms: u64,
    ) -> Result<String, CommandTokenError> {
        validate_generated_claims(purpose, command_name, scope, subject)?;
        let payload = serde_json::to_vec(&RuntimeTokenClaims {
            version: "v1".to_string(),
            purpose: purpose.to_string(),
            command_name: command_name.map(ToOwned::to_owned),
            scope: scope.map(ToOwned::to_owned),
            subject: subject.map(ToOwned::to_owned),
            issued_at_ms,
            expires_at_ms: issued_at_ms.saturating_add(ttl_ms),
            jti: None,
            request_hash: None,
        })
        .map_err(|_| CommandTokenError::InvalidFormat)?;
        let signature = self
            .provider
            .sign(&payload, &self.claims)
            .map_err(|_| CommandTokenError::Unauthorized)?;
        Ok(format!(
            "v1.{}.{}",
            encode_hex(&payload),
            encode_hex(&signature)
        ))
    }

    #[allow(clippy::too_many_arguments)]
    /// Issues a request-bound V1 token with optional replay identity.
    pub fn create_v1_with_jti_and_hash(
        &self,
        purpose: &str,
        command_name: Option<&str>,
        scope: Option<&str>,
        subject: Option<&str>,
        issued_at_ms: u64,
        ttl_ms: u64,
        jti: Option<String>,
        request_hash: Option<String>,
    ) -> Result<String, CommandTokenError> {
        validate_generated_claims(purpose, command_name, scope, subject)?;
        let payload = serde_json::to_vec(&RuntimeTokenClaims {
            version: "v1".to_string(),
            purpose: purpose.to_string(),
            command_name: command_name.map(ToOwned::to_owned),
            scope: scope.map(ToOwned::to_owned),
            subject: subject.map(ToOwned::to_owned),
            issued_at_ms,
            expires_at_ms: issued_at_ms.saturating_add(ttl_ms),
            jti,
            request_hash,
        })
        .map_err(|_| CommandTokenError::InvalidFormat)?;
        let signature = self
            .provider
            .sign(&payload, &self.claims)
            .map_err(|_| CommandTokenError::Unauthorized)?;
        Ok(format!(
            "v1.{}.{}",
            encode_hex(&payload),
            encode_hex(&signature)
        ))
    }
}

impl<'a, P: TokenProvider> CommandTokenValidator<'a, P> {
    /// Creates a bearer token validator.
    pub fn new(provider: &'a P, claims: TokenClaims) -> Self {
        Self { provider, claims }
    }

    /// Validates a command token for one command name.
    pub fn validate(
        &self,
        token: &str,
        command_name: &str,
        now_ms: u64,
    ) -> Result<(), CommandTokenError> {
        self.validate_for_purpose(token, "command", Some(command_name), now_ms)
    }

    /// Validates a token for an isolated purpose and optional resource name.
    pub fn validate_for_purpose(
        &self,
        token: &str,
        expected_purpose: &str,
        command_name: Option<&str>,
        now_ms: u64,
    ) -> Result<(), CommandTokenError> {
        self.validate_and_get_claims(token, expected_purpose, command_name, now_ms, None)?;
        Ok(())
    }

    /// Validates a token and returns its trusted claims.
    pub fn validate_and_get_claims(
        &self,
        token: &str,
        expected_purpose: &str,
        command_name: Option<&str>,
        now_ms: u64,
        expected_request_hash: Option<&str>,
    ) -> Result<RuntimeTokenClaims, CommandTokenError> {
        // Separação rígida de propósitos (command, query, sync) para evitar que um token de query rode comandos.
        if let Some((payload, signature)) = parse_v1_token(token) {
            self.provider
                .verify(&payload, &signature, &self.claims)
                .map_err(|_| CommandTokenError::Unauthorized)?;
            let claims = serde_json::from_slice::<RuntimeTokenClaims>(&payload)
                .map_err(|_| CommandTokenError::InvalidFormat)?;
            if claims.version != "v1" || claims.purpose != expected_purpose {
                return Err(CommandTokenError::Unauthorized);
            }
            if claims.expires_at_ms <= now_ms {
                return Err(CommandTokenError::Unauthorized);
            }
            validate_claim_scope(&claims, command_name)?;

            if let Some(hash) = &claims.request_hash {
                if let Some(expected) = expected_request_hash {
                    if hash != expected {
                        return Err(CommandTokenError::Forbidden);
                    }
                } else {
                    return Err(CommandTokenError::Unauthorized);
                }
            }

            return Ok(claims);
        }

        Err(CommandTokenError::InvalidFormat)
    }
}

fn validate_generated_claims(
    purpose: &str,
    command_name: Option<&str>,
    scope: Option<&str>,
    subject: Option<&str>,
) -> Result<(), CommandTokenError> {
    // O escopo coringa '*' é restrito a comandos/queries assinados pelo subject 'local-admin'.
    match purpose {
        "command" | "query" => match (command_name, scope) {
            (_, Some("*")) if subject == Some(LOCAL_ADMIN_SUBJECT) => Ok(()),
            (Some(_), None) => Ok(()),
            _ => Err(CommandTokenError::InvalidFormat),
        },
        "sync" if command_name.is_none() && scope.is_none() => Ok(()),
        "peer" => match (command_name, scope) {
            (_, Some("*")) if subject == Some(LOCAL_ADMIN_SUBJECT) => Ok(()),
            (None, None) => Ok(()),
            _ => Err(CommandTokenError::InvalidFormat),
        },
        _ => Err(CommandTokenError::InvalidFormat),
    }
}

fn validate_claim_scope(
    claims: &RuntimeTokenClaims,
    expected_name: Option<&str>,
) -> Result<(), CommandTokenError> {
    match claims.purpose.as_str() {
        "command" | "query" => {
            if claims.scope.as_deref() == Some("*") {
                return if claims.subject.as_deref() == Some(LOCAL_ADMIN_SUBJECT) {
                    Ok(())
                } else {
                    Err(CommandTokenError::Unauthorized)
                };
            }
            if claims.scope.is_some() {
                return Err(CommandTokenError::Unauthorized);
            }
            let name = claims
                .command_name
                .as_deref()
                .ok_or(CommandTokenError::Unauthorized)?;
            let expected_name = expected_name.ok_or(CommandTokenError::Unauthorized)?;
            if name != expected_name {
                return Err(CommandTokenError::Forbidden);
            }
            Ok(())
        }
        "sync" if claims.command_name.is_none() && claims.scope.is_none() => Ok(()),
        "peer" => {
            if claims.scope.as_deref() == Some("*") {
                return if claims.subject.as_deref() == Some(LOCAL_ADMIN_SUBJECT) {
                    Ok(())
                } else {
                    Err(CommandTokenError::Unauthorized)
                };
            }
            if claims.command_name.is_none() && claims.scope.is_none() {
                return Ok(());
            }
            Err(CommandTokenError::Unauthorized)
        }
        _ => Err(CommandTokenError::Unauthorized),
    }
}

fn parse_v1_token(token: &str) -> Option<(Vec<u8>, Vec<u8>)> {
    let token = token.strip_prefix("v1.")?;
    let (payload_hex, signature_hex) = token.split_once('.')?;
    let payload = decode_hex(payload_hex)?;
    let signature = decode_hex(signature_hex)?;
    Some((payload, signature))
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn decode_hex(input: &str) -> Option<Vec<u8>> {
    if input.is_empty() || !input.len().is_multiple_of(2) {
        return None;
    }
    let mut output = Vec::with_capacity(input.len() / 2);
    let bytes = input.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        let hi = hex_value(bytes[index])?;
        let lo = hex_value(bytes[index + 1])?;
        output.push((hi << 4) | lo);
        index += 2;
    }
    Some(output)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(10 + byte - b'a'),
        b'A'..=b'F' => Some(10 + byte - b'A'),
        _ => None,
    }
}

#[cfg(test)]
mod token_tests;
