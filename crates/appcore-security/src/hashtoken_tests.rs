// =============================================================================
//        #######
//     ###       ###     F: hashtoken_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::HashTokenProvider;
use crate::token::{SecurityError, TokenClaims, TokenProvider};
use crate::{new_rotated_secret, FileSecretKeyring};
use hash_token_rust::Algorithm;

fn claims() -> TokenClaims {
    TokenClaims {
        issuer: "runtime-a".to_string(),
        audience: "runtime-b".to_string(),
        salt: "salt-1".to_string(),
        ttl_ms: 2_000,
    }
}

fn provider() -> HashTokenProvider {
    HashTokenProvider::from_secret(b"explicit-test-secret".to_vec()).expect("provider")
}

#[test]
fn sign_and_verify_work() {
    let provider = provider();
    let payload = b"payload";
    let signature = provider.sign(payload, &claims());
    assert!(signature.is_ok());
    let signature = match signature {
        Ok(signature) => signature,
        Err(_) => return,
    };
    assert!(provider.verify(payload, &signature, &claims()).is_ok());
}

#[test]
fn seal_and_open_work() {
    let provider = provider();
    let payload = b"secret";
    let token = provider.seal(payload, &claims());
    assert!(token.is_ok());
    let token = match token {
        Ok(token) => token,
        Err(_) => return,
    };
    let opened = provider.open(&token, &claims());
    assert_eq!(opened.ok(), Some(payload.to_vec()));
}

#[test]
fn issuer_mismatch_fails() {
    let provider = provider();
    let payload = b"payload";
    let signature = provider.sign(payload, &claims());
    assert!(signature.is_ok());
    let signature = match signature {
        Ok(signature) => signature,
        Err(_) => return,
    };
    let wrong = TokenClaims {
        issuer: "wrong".to_string(),
        ..claims()
    };
    assert_eq!(
        provider.verify(payload, &signature, &wrong),
        Err(SecurityError::VerificationFailed)
    );
}

#[test]
fn explicit_secret_is_required() {
    assert!(matches!(
        HashTokenProvider::from_secret(Vec::new()),
        Err(SecurityError::InvalidToken)
    ));
}

#[test]
fn explicit_material_factories_apply_the_same_invariants() {
    let short = b"too-short".to_vec();
    let valid = b"explicit-test-secret".to_vec();
    let salt = vec![b"runtime".to_vec()];

    assert!(matches!(
        HashTokenProvider::with_secret(short.clone(), salt.clone()),
        Err(SecurityError::InvalidToken)
    ));
    assert!(matches!(
        HashTokenProvider::with_material(short, salt.clone(), Algorithm::Sha512),
        Err(SecurityError::InvalidToken)
    ));
    assert!(matches!(
        HashTokenProvider::with_secret(valid.clone(), Vec::new()),
        Err(SecurityError::InvalidToken)
    ));
    assert!(matches!(
        HashTokenProvider::with_material(valid.clone(), vec![Vec::new()], Algorithm::Sha256),
        Err(SecurityError::InvalidToken)
    ));
    assert!(HashTokenProvider::with_secret(valid.clone(), salt.clone()).is_ok());
    assert!(HashTokenProvider::with_material(valid, salt, Algorithm::Sha512).is_ok());
}

#[test]
fn keyring_rotation_keeps_existing_tokens_valid_without_restarting_provider() {
    let root = std::env::temp_dir().join(format!(
        "appcore-hashtoken-keyring-{}-{}",
        std::process::id(),
        super::unix_time_ms()
    ));
    let keyring = FileSecretKeyring::open(&root).unwrap();
    let first = new_rotated_secret(None).unwrap();
    keyring.install_initial(&first).unwrap();
    let provider = HashTokenProvider::from_keyring(
        keyring.clone(),
        vec![b"app-a".to_vec(), b"cluster-a".to_vec()],
    )
    .unwrap();
    let payload = b"rotation-payload";
    let first_signature = provider.sign(payload, &claims()).unwrap();

    let second = new_rotated_secret(None).unwrap();
    keyring.rotate(&second, super::unix_time_ms()).unwrap();
    assert!(provider
        .verify(payload, &first_signature, &claims())
        .is_ok());
    let second_signature = provider.sign(payload, &claims()).unwrap();
    assert_ne!(first_signature, second_signature);
    assert!(provider
        .verify(payload, &second_signature, &claims())
        .is_ok());

    keyring.revoke(&first.metadata.key_id).unwrap();
    assert_eq!(
        provider.verify(payload, &first_signature, &claims()),
        Err(SecurityError::VerificationFailed)
    );
    assert!(provider
        .verify(payload, &second_signature, &claims())
        .is_ok());
    std::fs::remove_dir_all(root).unwrap();
}
