// =============================================================================
//        #######
//     ###       ###     F: runtime_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================
// appcore-norm: test

use super::*;
use crate::ha::coordinator::tests::{coordinator, TestProvider};
use crate::GatewayHaMode;
use std::io::Write;
use std::net::TcpStream;
use std::time::{SystemTime, UNIX_EPOCH};

fn runtime(address: SocketAddr) -> GatewayRuntime {
    GatewayRuntime::new(
        GatewayConfig::new(address, "gateway.test"),
        token_provider(),
    )
    .unwrap()
}

#[test]
fn bind_failure_is_synchronous_and_fail_closed() {
    let occupied = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = occupied.local_addr().unwrap();
    let gateway = runtime(address);

    let error = gateway.start().unwrap_err();

    assert!(error
        .to_string()
        .contains("failed to bind gateway listener"));
    assert_eq!(gateway.snapshot().state, GatewayRuntimeState::Failed);
}

#[test]
fn shutdown_releases_listener_and_owned_runtime_thread() {
    let gateway = runtime("127.0.0.1:0".parse().unwrap());
    gateway.start().unwrap();
    let address = gateway.snapshot().bound_address.unwrap();
    assert!(TcpStream::connect_timeout(&address, Duration::from_secs(1)).is_ok());

    gateway.stop(Duration::from_secs(2)).unwrap();

    assert_eq!(gateway.snapshot().state, GatewayRuntimeState::Stopped);
    assert!(TcpListener::bind(address).is_ok());
}

#[test]
fn shutdown_force_closes_an_incomplete_http_connection_before_deadline() {
    let gateway = runtime("127.0.0.1:0".parse().unwrap());
    gateway.start().unwrap();
    let address = gateway.snapshot().bound_address.unwrap();
    let mut client = TcpStream::connect(address).unwrap();
    client
        .write_all(b"GET /v1/mesh-relay HTTP/1.1\r\nHost: gateway.test\r\n")
        .unwrap();
    std::thread::sleep(Duration::from_millis(100));

    let started = Instant::now();
    let _ = gateway.stop(Duration::from_millis(500));

    assert!(started.elapsed() < Duration::from_secs(1));
    assert_ne!(gateway.snapshot().state, GatewayRuntimeState::Orphaned);
    drop(client);
    assert!(TcpListener::bind(address).is_ok());
}

#[test]
fn ha_runtime_owns_coordinator_recovery_and_shutdown() {
    let coordinator = Arc::new(coordinator(
        Arc::new(TestProvider::default()),
        &["tenant-a"],
    ));
    let gateway = GatewayRuntime::with_ha_coordinator(
        GatewayConfig::new("127.0.0.1:0".parse().unwrap(), "gateway.test"),
        token_provider(),
        Arc::new(BoundedReplayStore::new(ReplayStoreConfig::default())),
        coordinator,
    )
    .unwrap();
    gateway.start().unwrap();

    let deadline = Instant::now() + Duration::from_secs(1);
    while gateway
        .snapshot()
        .ha
        .is_some_and(|ha| ha.lifecycle.mode != GatewayHaMode::Healthy)
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    let running = gateway.snapshot();
    assert_eq!(running.ha.unwrap().lifecycle.mode, GatewayHaMode::Healthy);

    gateway.stop(Duration::from_secs(2)).unwrap();
    let stopped = gateway.snapshot();
    assert_eq!(stopped.state, GatewayRuntimeState::Stopped);
    assert_eq!(stopped.ha.unwrap().lifecycle.mode, GatewayHaMode::Stopped);
}

fn token_provider() -> HashTokenProvider {
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos()
        .to_le_bytes();
    HashTokenProvider::from_secret(seed.repeat(2)).unwrap()
}
