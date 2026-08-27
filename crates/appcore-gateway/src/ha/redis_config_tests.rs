// =============================================================================
//        #######
//     ###       ###     F: redis_config_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.4-rc
// =============================================================================
// appcore-norm: test

use super::*;
use zeroize::Zeroizing;

#[test]
fn redis_config_requires_tls_away_from_loopback() {
    assert!(
        RedisGatewayRegistryConfig::new("redis://127.0.0.1:6379", "gateway", 2_000, 32).is_ok()
    );
    assert!(RedisGatewayRegistryConfig::new("redis://[::1]:6379/0", "gateway", 2_000, 32).is_ok());
    assert!(RedisGatewayRegistryConfig::new(
        "redis://cache.example.com:6379",
        "gateway",
        2_000,
        32,
    )
    .is_err());
    assert!(RedisGatewayRegistryConfig::new(
        "rediss://cache.example.com:6380/0",
        "gateway",
        2_000,
        32,
    )
    .is_ok());
}

#[test]
fn redis_config_rejects_credentials_and_redacts_endpoint() {
    assert!(RedisGatewayRegistryConfig::new(
        "rediss://user:secret@cache.example.com",
        "gateway",
        2_000,
        32,
    )
    .is_err());
    let config = RedisGatewayRegistryConfig::new(
        "rediss://private.example.com:6380",
        "gateway.prod",
        2_000,
        32,
    )
    .unwrap();
    assert!(!format!("{config:?}").contains("private.example.com"));
}

#[test]
fn redis_config_enforces_operation_bounds() {
    assert!(RedisGatewayRegistryConfig::new("redis://localhost", "gateway", 0, 32).is_err());
    assert!(RedisGatewayRegistryConfig::new("redis://localhost", "gateway", 5_001, 32).is_err());
    assert!(RedisGatewayRegistryConfig::new("redis://localhost", "gateway", 2_000, 0).is_err());
    assert!(RedisGatewayRegistryConfig::new("redis://localhost", "gateway", 2_000, 65).is_err());
    assert!(RedisGatewayRegistryConfig::new("redis://localhost", "bad:{slot}", 2_000, 32).is_err());
}

#[test]
fn redis_credential_is_non_empty_and_redacted() {
    assert!(RedisGatewayCredential::new(Zeroizing::new(String::new())).is_err());
    let material = format!(
        "ephemeral-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let credential = RedisGatewayCredential::new(Zeroizing::new(material.clone())).unwrap();
    assert_eq!(
        format!("{credential:?}"),
        "RedisGatewayCredential(REDACTED)"
    );
    assert!(!format!("{credential:?}").contains(&material));
}
