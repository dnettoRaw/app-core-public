// =============================================================================
//        #######
//     ###       ###     F: auth_server_network_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/07 12:31:50 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 13:18:47 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;
use appcore_security::{HashTokenProvider, TokenProvider};
use appcore_storage::{
    data_claims, make_auth_request, now_ms, open_remote_response, seal_remote_request,
    AuthRemoteRequest, AUTH_REMOTE_SCHEMA,
};
use appcore_supervisor::{PassiveManagedService, ServiceActivationState};

fn provider(secret: &[u8]) -> HashTokenProvider {
    HashTokenProvider::from_secret(secret.to_vec()).expect("provider")
}

fn transport_provider() -> HashTokenProvider {
    provider(b"transport-secret-1234567890")
}

fn data_provider() -> HashTokenProvider {
    provider(b"data-secret-123456789012345")
}

#[test]
fn valid_request_returns_sealed_payload_without_data_secret_leak() {
    let transport = transport_provider();
    let data = data_provider();
    let replay = AuthReplayCache::default();
    let request = make_auth_request("private.bin", "seal", b"payload", now_ms()).expect("request");
    let token = seal_remote_request(&request, &transport).expect("token");

    let response = handle_auth_storage_token(&token, &transport, &data, &replay, now_ms())
        .expect("response token");
    let sealed = open_remote_response(&response, &transport, &request.nonce, now_ms())
        .expect("sealed payload");
    let opened = data.open(&sealed, &data_claims()).expect("open data");

    assert_eq!(opened, b"payload");
    assert_ne!(sealed, b"payload".to_vec());
}

#[test]
fn replayed_nonce_is_rejected() {
    let transport = transport_provider();
    let data = data_provider();
    let replay = AuthReplayCache::default();
    let request = make_auth_request("private.bin", "seal", b"payload", now_ms()).expect("request");
    let token = seal_remote_request(&request, &transport).expect("token");

    assert!(handle_auth_storage_token(&token, &transport, &data, &replay, now_ms()).is_ok());
    let replayed = handle_auth_storage_token(&token, &transport, &data, &replay, now_ms());

    assert!(matches!(replayed, Err(error) if error.status == 409));
}

#[test]
fn replay_cache_expires_entries_and_exposes_bounded_metrics() {
    let config = ReplayStoreConfig::new(1, 10, 1).unwrap();
    let replay = AuthReplayCache::with_config(config);
    let transport = transport_provider();
    let data = data_provider();
    let first = make_auth_request("private.bin", "seal", b"payload", 1).expect("request");
    let first_token = seal_remote_request(&first, &transport).expect("token");

    assert!(handle_auth_storage_token(&first_token, &transport, &data, &replay, 1).is_ok());
    assert_eq!(replay.metrics().entries, 1);

    let second = make_auth_request("private.bin", "seal", b"payload", 20).expect("request");
    let second_token = seal_remote_request(&second, &transport).expect("token");
    assert!(handle_auth_storage_token(&second_token, &transport, &data, &replay, 20).is_ok());
    assert_eq!(replay.metrics().entries, 1);
    assert_eq!(replay.metrics().expired, 1);
}

#[test]
fn rate_limiter_enforces_window_and_recovers() {
    let limiter = RateLimiter::new(2, 100);

    assert!(limiter.allow(1));
    assert!(limiter.allow(2));
    assert!(!limiter.allow(3));
    assert!(limiter.allow(101));
}

#[test]
fn runtime_managed_auth_uses_the_global_supervisor() {
    let global = Arc::new(Supervisor::new());
    let placeholder = ServiceDescriptor::new(
        "auth-server",
        ManagedResource::AuthServer,
        RestartPolicy::never(),
    )
    .unwrap()
    .with_activation(ServiceActivationState::NotConfigured);
    global
        .register(Arc::new(PassiveManagedService::new(placeholder)))
        .unwrap();
    let (selected, standalone) =
        supervisor_for_hosting(AuthServerHosting::RuntimeManaged(Arc::clone(&global))).unwrap();
    let replacement = ServiceDescriptor::new(
        "auth-server",
        ManagedResource::AuthServer,
        RestartPolicy::never(),
    )
    .unwrap();
    start_hosted_auth_service(
        &selected,
        Arc::new(PassiveManagedService::new(replacement)),
        standalone,
    )
    .unwrap();

    assert!(!standalone);
    assert!(selected.same_instance(global.as_ref()));
    assert!(selected.snapshots()[0].enabled);
}

#[test]
fn companion_auth_owns_a_distinct_supervisor() {
    let global = Supervisor::new();
    let (selected, standalone) =
        supervisor_for_hosting(AuthServerHosting::StandaloneCompanion).unwrap();

    assert!(standalone);
    assert!(!selected.same_instance(&global));
    selected.shutdown(now_ms()).unwrap();
}

#[test]
fn expired_remote_request_is_rejected() {
    let transport = transport_provider();
    let data = data_provider();
    let replay = AuthReplayCache::default();
    let request = expired_request();
    let token = seal_remote_request(&request, &transport).expect("token");

    let result = handle_auth_storage_token(&token, &transport, &data, &replay, 100);

    assert!(matches!(result, Err(error) if error.status == 401));
}

#[test]
fn wrong_transport_secret_is_rejected() {
    let good_transport = transport_provider();
    let wrong_transport = provider(b"wrong-transport-secret-123");
    let data = data_provider();
    let replay = AuthReplayCache::default();
    let request = make_auth_request("private.bin", "seal", b"payload", now_ms()).expect("request");
    let token = seal_remote_request(&request, &wrong_transport).expect("token");

    let result = handle_auth_storage_token(&token, &good_transport, &data, &replay, now_ms());

    assert!(matches!(result, Err(error) if error.status == 401));
}

#[test]
fn wrong_data_secret_does_not_open_sensitive_payload() {
    let transport = transport_provider();
    let good_data = data_provider();
    let wrong_data = provider(b"wrong-data-secret-123456789");
    let replay = AuthReplayCache::default();
    let sealed = good_data
        .seal(b"payload", &data_claims())
        .expect("data sealed");
    let request = make_auth_request("private.bin", "open", &sealed, now_ms()).expect("request");
    let token = seal_remote_request(&request, &transport).expect("token");

    let result = handle_auth_storage_token(&token, &transport, &wrong_data, &replay, now_ms());

    assert!(matches!(result, Err(error) if error.status == 403));
}

fn expired_request() -> AuthRemoteRequest {
    AuthRemoteRequest {
        schema: AUTH_REMOTE_SCHEMA.to_string(),
        resource: "private.bin".to_string(),
        operation: "seal".to_string(),
        nonce: "expired-nonce".to_string(),
        issued_at_ms: 1,
        expires_at_ms: 2,
        payload_hex: "7061796c6f6164".to_string(),
    }
}
