// =============================================================================
//        #######
//     ###       ###     F: discovery_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/05 15:26:21 by dnettoRaw
//    ##   ## ##   ##    U: 2026/06/05 15:26:21 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::{discover_dns_sync_peers, SyncPeerAddress, SyncPeerScheme};
use crate::sync::SyncError;

#[test]
fn parses_static_http_peer() {
    let peer = SyncPeerAddress::parse("http://127.0.0.1:39211");
    assert!(peer.is_ok());
    let peer = peer.expect("peer should parse");
    assert_eq!(peer.host, "127.0.0.1");
    assert_eq!(peer.port, 39211);
    assert_eq!(peer.url, "http://127.0.0.1:39211");
    assert_eq!(peer.scheme, SyncPeerScheme::Http);
}

#[test]
fn parses_static_https_peer() {
    let peer = SyncPeerAddress::parse("https://meunomedeappexemplo.local:443");
    assert!(peer.is_ok());
    let peer = peer.expect("peer should parse");
    assert_eq!(peer.host, "meunomedeappexemplo.local");
    assert_eq!(peer.port, 443);
    assert_eq!(peer.url, "https://meunomedeappexemplo.local:443");
    assert_eq!(peer.scheme, SyncPeerScheme::Https);
}

#[test]
fn parses_bracketed_ipv6_peer() {
    let peer = SyncPeerAddress::parse("http://[::1]:39211");
    assert!(peer.is_ok());
    let peer = peer.expect("peer should parse");
    assert_eq!(peer.host, "::1");
    assert_eq!(peer.port, 39211);
    assert_eq!(peer.url, "http://[::1]:39211");
}

#[test]
fn rejects_unsupported_scheme() {
    let peer = SyncPeerAddress::parse("ftp://localhost:39211");
    assert_eq!(peer, Err(SyncError::UnsupportedPeerScheme));
}

#[test]
fn rejects_peer_without_port() {
    let peer = SyncPeerAddress::parse("localhost");
    assert_eq!(peer, Err(SyncError::InvalidPeerAddress));
}

#[test]
fn dns_discovery_resolves_localhost_seed() {
    let seeds = vec!["localhost:39211".to_string()];
    let peers = discover_dns_sync_peers(&seeds, 39201);
    assert!(peers.is_ok());
    let peers = peers.expect("localhost should resolve");
    assert!(!peers.is_empty());
    assert!(peers.iter().all(|peer| peer.port == 39211));
}

#[test]
fn dns_discovery_uses_default_port() {
    let seeds = vec!["localhost".to_string()];
    let peers = discover_dns_sync_peers(&seeds, 39222);
    assert!(peers.is_ok());
    let peers = peers.expect("localhost should resolve");
    assert!(!peers.is_empty());
    assert!(peers.iter().all(|peer| peer.port == 39222));
}

#[test]
fn dns_discovery_preserves_https_seed_name_for_tls_sni() {
    let seeds = vec!["https://localhost".to_string()];
    let peers = discover_dns_sync_peers(&seeds, 39222);
    assert!(peers.is_ok());
    let peers = peers.expect("localhost should resolve");
    assert_eq!(peers[0].host, "localhost");
    assert_eq!(peers[0].scheme, SyncPeerScheme::Https);
    assert_eq!(peers[0].url, "https://localhost:39222");
}
