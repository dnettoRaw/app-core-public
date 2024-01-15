// =============================================================================
//        #######
//     ###       ###     F: token_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:42:05 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::{
    encode_hex, CommandTokenError, CommandTokenFactory, CommandTokenValidator, RuntimeTokenClaims,
    SecurityResult, TokenClaims, TokenProvider, LOCAL_ADMIN_SUBJECT,
};

struct MockTokenProvider;

impl TokenProvider for MockTokenProvider {
    fn seal(&self, payload: &[u8], _claims: &TokenClaims) -> SecurityResult<Vec<u8>> {
        Ok(payload.to_vec())
    }

    fn open(&self, token: &[u8], _claims: &TokenClaims) -> SecurityResult<Vec<u8>> {
        Ok(token.to_vec())
    }

    fn sign(&self, payload: &[u8], _claims: &TokenClaims) -> SecurityResult<Vec<u8>> {
        Ok(payload.to_vec())
    }

    fn verify(
        &self,
        payload: &[u8],
        signature: &[u8],
        _claims: &TokenClaims,
    ) -> SecurityResult<()> {
        if payload == signature {
            return Ok(());
        }
        Err(super::SecurityError::VerificationFailed)
    }
}

fn claims() -> TokenClaims {
    TokenClaims {
        issuer: "runtime-a".to_string(),
        audience: "runtime-b".to_string(),
        salt: "salt-1".to_string(),
        ttl_ms: 60_000,
    }
}

fn token_for_claims(
    provider: &MockTokenProvider,
    claims: &TokenClaims,
    token_claims: &RuntimeTokenClaims,
) -> String {
    let payload = serde_json::to_vec(token_claims).unwrap_or_default();
    let signature = provider.sign(&payload, claims).unwrap_or_default();
    format!("v1.{}.{}", encode_hex(&payload), encode_hex(&signature))
}

#[test]
fn mock_provider_roundtrip_works() {
    let provider = MockTokenProvider;
    let input = b"internal-command";

    let sealed = provider.seal(input, &claims());
    assert!(sealed.is_ok());
    let sealed = match sealed {
        Ok(sealed) => sealed,
        Err(_) => return,
    };
    let opened = provider.open(&sealed, &claims());
    assert!(opened.is_ok());
    let opened = match opened {
        Ok(opened) => opened,
        Err(_) => return,
    };
    assert_eq!(opened, input.to_vec());
}

#[test]
fn factory_generates_v1_token_and_validator_accepts() {
    let provider = MockTokenProvider;
    let claims = claims();
    let factory = CommandTokenFactory::new(&provider, claims.clone());
    let token = factory.create_v1(Some("runtime.ping"), None, 1000, 5000);
    assert!(token.is_ok());
    let token = match token {
        Ok(token) => token,
        Err(_) => return,
    };
    let validator = CommandTokenValidator::new(&provider, claims);
    assert!(validator.validate(&token, "runtime.ping", 2000).is_ok());
}

#[test]
fn validator_rejects_expired() {
    let provider = MockTokenProvider;
    let claims = claims();
    let factory = CommandTokenFactory::new(&provider, claims.clone());
    let token = factory.create_v1(Some("runtime.ping"), None, 1000, 10);
    assert!(token.is_ok());
    let token = match token {
        Ok(token) => token,
        Err(_) => return,
    };
    let validator = CommandTokenValidator::new(&provider, claims);
    assert_eq!(
        validator.validate(&token, "runtime.ping", 5000),
        Err(CommandTokenError::Unauthorized)
    );
}

#[test]
fn validator_rejects_command_mismatch() {
    let provider = MockTokenProvider;
    let claims = claims();
    let factory = CommandTokenFactory::new(&provider, claims.clone());
    let token = factory.create_v1(Some("runtime.other"), None, 1000, 5000);
    assert!(token.is_ok());
    let token = match token {
        Ok(token) => token,
        Err(_) => return,
    };
    let validator = CommandTokenValidator::new(&provider, claims);
    assert_eq!(
        validator.validate(&token, "runtime.ping", 2000),
        Err(CommandTokenError::Forbidden)
    );
}

#[test]
fn validator_rejects_without_command_name() {
    let provider = MockTokenProvider;
    let claims = claims();
    let token = token_for_claims(
        &provider,
        &claims,
        &RuntimeTokenClaims {
            version: "v1".to_string(),
            purpose: "command".to_string(),
            command_name: None,
            scope: None,
            subject: None,
            issued_at_ms: 1000,
            expires_at_ms: 6000,
            jti: None,
            request_hash: None,
        },
    );
    let validator = CommandTokenValidator::new(&provider, claims);
    assert_eq!(
        validator.validate(&token, "runtime.ping", 2000),
        Err(CommandTokenError::Unauthorized)
    );
}

#[test]
fn validator_accepts_explicit_wildcard_command_scope() {
    let provider = MockTokenProvider;
    let claims = claims();
    let factory = CommandTokenFactory::new(&provider, claims.clone());
    let token = factory.create_v1_scoped(None, Some("*"), Some(LOCAL_ADMIN_SUBJECT), 1000, 5000);
    assert!(token.is_ok());
    let validator = CommandTokenValidator::new(&provider, claims);
    assert!(validator
        .validate(&token.unwrap_or_default(), "runtime.ping", 2000)
        .is_ok());
}

#[test]
fn validator_rejects_wildcard_without_local_admin_subject() {
    let provider = MockTokenProvider;
    let claims = claims();
    let token = token_for_claims(
        &provider,
        &claims,
        &RuntimeTokenClaims {
            version: "v1".to_string(),
            purpose: "command".to_string(),
            command_name: None,
            scope: Some("*".to_string()),
            subject: None,
            issued_at_ms: 1000,
            expires_at_ms: 6000,
            jti: None,
            request_hash: None,
        },
    );
    let validator = CommandTokenValidator::new(&provider, claims);
    assert_eq!(
        validator.validate(&token, "runtime.ping", 2000),
        Err(CommandTokenError::Unauthorized)
    );
}

#[test]
fn validator_rejects_query_without_name() {
    let provider = MockTokenProvider;
    let claims = claims();
    let token = token_for_claims(
        &provider,
        &claims,
        &RuntimeTokenClaims {
            version: "v1".to_string(),
            purpose: "query".to_string(),
            command_name: None,
            scope: None,
            subject: None,
            issued_at_ms: 1000,
            expires_at_ms: 6000,
            jti: None,
            request_hash: None,
        },
    );
    let validator = CommandTokenValidator::new(&provider, claims);
    assert_eq!(
        validator.validate_for_purpose(&token, "query", Some("runtime.status"), 2000),
        Err(CommandTokenError::Unauthorized)
    );
}

#[test]
fn validator_accepts_explicit_wildcard_query_scope() {
    let provider = MockTokenProvider;
    let claims = claims();
    let factory = CommandTokenFactory::new(&provider, claims.clone());
    let token = factory.create_v1_for_purpose_scoped(
        "query",
        None,
        Some("*"),
        Some(LOCAL_ADMIN_SUBJECT),
        1000,
        5000,
    );
    assert!(token.is_ok());
    let validator = CommandTokenValidator::new(&provider, claims);
    assert!(validator
        .validate_for_purpose(
            &token.unwrap_or_default(),
            "query",
            Some("runtime.status"),
            2000,
        )
        .is_ok());
}

#[test]
fn validator_rejects_wrong_purpose() {
    let provider = MockTokenProvider;
    let claims = claims();
    let payload = serde_json::to_vec(&RuntimeTokenClaims {
        version: "v1".to_string(),
        purpose: "session".to_string(),
        command_name: Some("runtime.ping".to_string()),
        scope: None,
        subject: None,
        issued_at_ms: 1000,
        expires_at_ms: 6000,
        jti: None,
        request_hash: None,
    });
    assert!(payload.is_ok());
    let payload = match payload {
        Ok(payload) => payload,
        Err(_) => return,
    };
    let signature = provider.sign(&payload, &claims);
    assert!(signature.is_ok());
    let signature = match signature {
        Ok(signature) => signature,
        Err(_) => return,
    };
    let token = format!("v1.{}.{}", encode_hex(&payload), encode_hex(&signature));
    let validator = CommandTokenValidator::new(&provider, claims);
    assert_eq!(
        validator.validate(&token, "runtime.ping", 2000),
        Err(CommandTokenError::Unauthorized)
    );
}

#[test]
fn validator_accepts_sync_purpose_for_sync() {
    let provider = MockTokenProvider;
    let claims = claims();
    let factory = CommandTokenFactory::new(&provider, claims.clone());
    let token = factory.create_v1_for_purpose("sync", None, None, 1000, 5000);
    assert!(token.is_ok());
    let token = match token {
        Ok(token) => token,
        Err(_) => return,
    };
    let validator = CommandTokenValidator::new(&provider, claims);
    assert!(validator
        .validate_for_purpose(&token, "sync", None, 2000)
        .is_ok());
}

#[test]
fn validator_rejects_sync_token_for_command() {
    let provider = MockTokenProvider;
    let claims = claims();
    let factory = CommandTokenFactory::new(&provider, claims.clone());
    let token = factory.create_v1_for_purpose("sync", None, None, 1000, 5000);
    assert!(token.is_ok());
    let token = match token {
        Ok(token) => token,
        Err(_) => return,
    };
    let validator = CommandTokenValidator::new(&provider, claims);
    assert_eq!(
        validator.validate(&token, "runtime.ping", 2000),
        Err(CommandTokenError::Unauthorized)
    );
}

#[test]
fn test_token_request_hash_validation() {
    use super::{compute_request_hash, RequestValidationDetails};
    let provider = MockTokenProvider;
    let claims = claims();
    let factory = CommandTokenFactory::new(&provider, claims.clone());

    let details = RequestValidationDetails {
        purpose: "command".to_string(),
        name: "runtime.ping".to_string(),
        id: "cmd-123".to_string(),
        idempotency_key: Some("key-456".to_string()),
        payload: "hello".to_string(),
        subject: None,
        audience: None,
    };
    let hash = compute_request_hash(&details);

    // Create token with correct hash
    let token = factory
        .create_v1_with_jti_and_hash(
            "command",
            Some("runtime.ping"),
            None,
            None,
            1000,
            5000,
            None,
            Some(hash.clone()),
        )
        .unwrap();

    let validator = CommandTokenValidator::new(&provider, claims.clone());

    let res = validator.validate_and_get_claims(
        &token,
        "command",
        Some("runtime.ping"),
        2000,
        Some(&hash),
    );
    assert!(res.is_ok());

    let mismatch_hash = "wronghash123".to_string();
    let res2 = validator.validate_and_get_claims(
        &token,
        "command",
        Some("runtime.ping"),
        2000,
        Some(&mismatch_hash),
    );
    assert_eq!(res2, Err(CommandTokenError::Forbidden));

    let res3 =
        validator.validate_and_get_claims(&token, "command", Some("runtime.ping"), 2000, None);
    assert_eq!(res3, Err(CommandTokenError::Unauthorized));
}

#[test]
fn request_hash_v2_has_a_stable_test_vector() {
    use super::{compute_request_hash, RequestValidationDetails};
    let details = RequestValidationDetails {
        purpose: "command".to_string(),
        name: "runtime.ping".to_string(),
        id: "cmd-123".to_string(),
        idempotency_key: Some("key-456".to_string()),
        payload: "hello".to_string(),
        subject: None,
        audience: None,
    };

    assert_eq!(
        compute_request_hash(&details),
        "v2:d7ef89f147a8a7544fdbaf3d47bbc9cc6efd1f1d58904ca5c38100003c15f7f1"
    );
}

#[test]
fn request_hash_v2_distinguishes_structure_and_optional_presence() {
    use super::{compute_request_hash, RequestValidationDetails};
    let first = RequestValidationDetails {
        purpose: "a|b".to_string(),
        name: "c".to_string(),
        id: "request".to_string(),
        idempotency_key: None,
        payload: String::new(),
        subject: None,
        audience: None,
    };
    let mut second = first.clone();
    second.purpose = "a".to_string();
    second.name = "b|c".to_string();
    assert_ne!(compute_request_hash(&first), compute_request_hash(&second));

    let mut present_empty = first.clone();
    present_empty.idempotency_key = Some(String::new());
    assert_ne!(
        compute_request_hash(&first),
        compute_request_hash(&present_empty)
    );
    assert!(compute_request_hash(&first).starts_with("v2:"));
}
