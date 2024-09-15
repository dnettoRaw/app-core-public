// =============================================================================
//        #######
//     ###       ###     F: discovery.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/05 15:26:21 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! DNS peer discovery helpers for conservative sync push.

use crate::sync::error::{SyncError, SyncResult};
use std::collections::BTreeSet;
use std::net::ToSocketAddrs;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
/// Parsed and normalized address of a synchronization peer.
pub struct SyncPeerAddress {
    /// Canonical URL including scheme, host, and port.
    pub url: String,
    /// DNS name or IP literal without brackets.
    pub host: String,
    /// TCP port used by the synchronization endpoint.
    pub port: u16,
    /// HTTP transport scheme.
    pub scheme: SyncPeerScheme,
}

impl SyncPeerAddress {
    /// Parses a peer URL or explicit host-and-port seed.
    pub fn parse(peer: &str) -> SyncResult<Self> {
        let parsed = parse_peer_seed(peer, None)?;
        Ok(Self::from_host_port(
            parsed.host,
            parsed.port,
            parsed.scheme,
        ))
    }

    fn from_host_port(host: String, port: u16, scheme: SyncPeerScheme) -> Self {
        let scheme_text = scheme.as_str();
        let url = if host.contains(':') {
            format!("{scheme_text}://[{host}]:{port}")
        } else {
            format!("{scheme_text}://{host}:{port}")
        };
        Self {
            url,
            host,
            port,
            scheme,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
/// HTTP scheme supported by the built-in synchronization transport.
pub enum SyncPeerScheme {
    /// Plain HTTP, intended for trusted local or externally secured networks.
    Http,
    /// HTTPS with server certificate validation.
    Https,
}

impl SyncPeerScheme {
    /// Returns the lowercase URI scheme.
    pub fn as_str(self) -> &'static str {
        match self {
            SyncPeerScheme::Http => "http",
            SyncPeerScheme::Https => "https",
        }
    }
}

/// Resolves DNS peer seeds and returns normalized, de-duplicated addresses.
pub fn discover_dns_sync_peers(
    seeds: &[String],
    default_port: u16,
) -> SyncResult<Vec<SyncPeerAddress>> {
    let mut peers = BTreeSet::new();
    for seed in seeds {
        let parsed = parse_peer_seed(seed, Some(default_port))?;
        ensure_resolves(&parsed.host, parsed.port)?;
        let _ = peers.insert(SyncPeerAddress::from_host_port(
            parsed.host,
            parsed.port,
            parsed.scheme,
        ));
    }
    Ok(peers.into_iter().collect())
}

fn ensure_resolves(host: &str, port: u16) -> SyncResult<()> {
    let addrs = (host, port)
        .to_socket_addrs()
        .map_err(|err| SyncError::DnsResolutionFailed(err.to_string()))?
        .collect::<Vec<_>>();
    if addrs.is_empty() {
        return Err(SyncError::DnsResolutionFailed(
            "sync dns seed resolved to no addresses".to_string(),
        ));
    }
    Ok(())
}

struct ParsedPeerSeed {
    host: String,
    port: u16,
    scheme: SyncPeerScheme,
}

fn parse_peer_seed(peer: &str, default_port: Option<u16>) -> SyncResult<ParsedPeerSeed> {
    let (scheme, address) = split_scheme(peer)?;
    if address.is_empty() {
        return Err(SyncError::InvalidPeerAddress);
    }
    if let Some((host, port)) = parse_bracketed_ipv6(address)? {
        return Ok(ParsedPeerSeed { host, port, scheme });
    }
    let (host, port) = parse_plain_host_port(address, default_port)?;
    Ok(ParsedPeerSeed { host, port, scheme })
}

fn split_scheme(peer: &str) -> SyncResult<(SyncPeerScheme, &str)> {
    let peer = peer.trim().trim_end_matches('/');
    if let Some(address) = peer.strip_prefix("http://") {
        return Ok((SyncPeerScheme::Http, address));
    }
    if let Some(address) = peer.strip_prefix("https://") {
        return Ok((SyncPeerScheme::Https, address));
    }
    if peer.contains("://") {
        return Err(SyncError::UnsupportedPeerScheme);
    }
    Ok((SyncPeerScheme::Http, peer))
}

fn parse_bracketed_ipv6(peer: &str) -> SyncResult<Option<(String, u16)>> {
    if !peer.starts_with('[') {
        return Ok(None);
    }
    let Some((host, rest)) = peer[1..].split_once(']') else {
        return Err(SyncError::InvalidPeerAddress);
    };
    let Some(port) = rest.strip_prefix(':') else {
        return Err(SyncError::InvalidPeerAddress);
    };
    Ok(Some((host.to_string(), parse_port(port)?)))
}

fn parse_plain_host_port(peer: &str, default_port: Option<u16>) -> SyncResult<(String, u16)> {
    let mut parts = peer.rsplitn(2, ':');
    let last = parts.next().unwrap_or_default();
    let host = parts.next();
    match (host, default_port) {
        (Some(host), _) if !host.is_empty() => Ok((host.to_string(), parse_port(last)?)),
        (None, Some(port)) => Ok((last.to_string(), port)),
        _ => Err(SyncError::InvalidPeerAddress),
    }
}

fn parse_port(raw: &str) -> SyncResult<u16> {
    raw.parse::<u16>()
        .map_err(|_| SyncError::InvalidPeerAddress)
}

#[cfg(test)]
#[path = "discovery_tests.rs"]
mod tests;
