// =============================================================================
//        #######
//     ###       ###     F: limit_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/05 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/05 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Boundary tests use a probe provider to observe crypto admission, not security.
// appcore-norm: test

use std::cell::Cell;

use super::*;

#[derive(Default)]
struct ProbeProvider {
    signs: Cell<usize>,
    verifies: Cell<usize>,
    signature_size: usize,
}

impl TokenProvider for ProbeProvider {
    fn seal(&self, _: &[u8], _: &TokenClaims) -> SecurityResult<Vec<u8>> {
        Err(SecurityError::Unsupported("test probe"))
    }

    fn open(&self, _: &[u8], _: &TokenClaims) -> SecurityResult<Vec<u8>> {
        Err(SecurityError::Unsupported("test probe"))
    }

    fn sign(&self, _: &[u8], _: &TokenClaims) -> SecurityResult<Vec<u8>> {
        self.signs.set(self.signs.get() + 1);
        Ok(vec![0; self.signature_size])
    }

    fn verify(&self, _: &[u8], _: &[u8], _: &TokenClaims) -> SecurityResult<()> {
        self.verifies.set(self.verifies.get() + 1);
        Ok(())
    }
}

fn provider_claims() -> TokenClaims {
    TokenClaims {
        issuer: "issuer".into(),
        audience: "audience".into(),
        salt: "label".into(),
        ttl_ms: 1000,
    }
}

#[test]
fn oversized_components_never_reach_crypto() {
    let provider = ProbeProvider::default();
    let validator = CommandTokenValidator::new(&provider, provider_claims());
    for token in [
        "x".repeat(MAX_RUNTIME_TOKEN_BYTES + 1),
        format!("v1.{}.00", "00".repeat(MAX_RUNTIME_TOKEN_PAYLOAD_BYTES + 1)),
        format!(
            "v1.00.{}",
            "00".repeat(MAX_RUNTIME_TOKEN_SIGNATURE_BYTES + 1)
        ),
    ] {
        assert_eq!(
            validator.validate(&token, "ping", 1),
            Err(CommandTokenError::InvalidFormat)
        );
    }
    assert_eq!(provider.verifies.get(), 0);
}

#[test]
fn exact_component_limits_accept_valid_claims() {
    let provider = ProbeProvider::default();
    let validator = CommandTokenValidator::new(&provider, provider_claims());
    let mut payload = br#"{"version":"v1","purpose":"command","command_name":"ping","issued_at_ms":0,"expires_at_ms":10}"#.to_vec();
    // JSON permits trailing whitespace; this exercises exact decoded size.
    payload.resize(MAX_RUNTIME_TOKEN_PAYLOAD_BYTES, b' ');
    let token = format!(
        "v1.{}.{}",
        encode_hex(&payload),
        "00".repeat(MAX_RUNTIME_TOKEN_SIGNATURE_BYTES)
    );
    assert_eq!(token.len(), MAX_RUNTIME_TOKEN_BYTES);
    assert_eq!(validator.validate(&token, "ping", 1), Ok(()));
    assert_eq!(provider.verifies.get(), 1);
}

#[test]
fn generation_bounds_fields_and_json_expansion_before_signing() {
    let provider = ProbeProvider::default();
    let factory = CommandTokenFactory::new(&provider, provider_claims());
    for subject in [
        "a".repeat(MAX_RUNTIME_TOKEN_PAYLOAD_BYTES + 1),
        "\0".repeat(MAX_RUNTIME_TOKEN_PAYLOAD_BYTES / 2),
    ] {
        assert_eq!(
            factory.create_v1(Some("ping"), Some(&subject), 0, 10),
            Err(CommandTokenError::InvalidFormat)
        );
    }
    assert_eq!(
        factory.create_v1_with_jti_and_hash(
            "command",
            Some("ping"),
            None,
            None,
            0,
            10,
            Some("a".repeat(MAX_RUNTIME_TOKEN_PAYLOAD_BYTES + 1)),
            None
        ),
        Err(CommandTokenError::InvalidFormat)
    );
    assert_eq!(provider.signs.get(), 0);
}

#[test]
fn generation_rejects_empty_and_oversized_provider_signatures() {
    for signature_size in [0, MAX_RUNTIME_TOKEN_SIGNATURE_BYTES + 1] {
        let provider = ProbeProvider {
            signature_size,
            ..Default::default()
        };
        let factory = CommandTokenFactory::new(&provider, provider_claims());
        assert_eq!(
            factory.create_v1(Some("ping"), None, 0, 10),
            Err(CommandTokenError::InvalidFormat)
        );
        assert_eq!(
            factory.create_v1_with_jti_and_hash(
                "command",
                Some("ping"),
                None,
                None,
                0,
                10,
                None,
                None
            ),
            Err(CommandTokenError::InvalidFormat)
        );
    }
}

#[test]
fn real_provider_roundtrips_large_unicode_claims() {
    let mut secret = vec![0; 32];
    getrandom::fill(&mut secret).unwrap();
    let provider = crate::HashTokenProvider::from_secret(secret).unwrap();
    let claims = provider_claims();
    let factory = CommandTokenFactory::new(&provider, claims.clone());
    let subject = "é日本語مرحبا".repeat(2000);
    let token = factory
        .create_v1(Some("ping"), Some(&subject), 0, 10)
        .unwrap();
    let validator = CommandTokenValidator::new(&provider, claims);
    let parsed = validator
        .validate_and_get_claims(&token, "command", Some("ping"), 1, None)
        .unwrap();
    assert_eq!(parsed.subject.as_deref(), Some(subject.as_str()));
}
