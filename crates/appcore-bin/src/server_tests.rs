// =============================================================================
//        #######
//     ###       ###     F: server_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 11:51:10 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;
use appcore_api::CommandTokenVerifier;
use appcore_security::{CommandTokenError, CommandTokenFactory, HashTokenProvider, TokenClaims};
use std::sync::Arc;

#[test]
fn test_server_verifier_jti_replay_protection() {
    let provider =
        HashTokenProvider::from_secret(b"secret-key-12345678-secret-key-12345678".to_vec())
            .unwrap();
    let claims = TokenClaims {
        issuer: "issuer".to_string(),
        audience: "audience".to_string(),
        salt: "command".to_string(),
        ttl_ms: 60_000,
    };
    let verifier = RuntimeCommandTokenVerifier {
        provider: provider.clone(),
        claims: claims.clone(),
        replay_store: Arc::new(appcore_peer_rpc::InMemoryPeerNonceStore::default()),
    };

    let factory = CommandTokenFactory::new(&provider, claims);
    let jti = "unique-jti-111".to_string();

    let token = factory
        .create_v1_with_jti_and_hash(
            "command",
            Some("runtime.ping"),
            None,
            None,
            now_ms(),
            60_000,
            Some(jti),
            None,
        )
        .unwrap();

    // First verification should succeed
    let res = verifier.verify_command_token_with_request(&token, "runtime.ping", None);
    assert!(res.is_ok());

    // Second verification with the same token/JTI should fail
    let res2 = verifier.verify_command_token_with_request(&token, "runtime.ping", None);
    assert_eq!(res2, Err(CommandTokenError::Unauthorized));
}
