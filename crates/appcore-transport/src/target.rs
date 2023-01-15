// =============================================================================
//        #######
//     ###       ###     F: target.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use crate::{TransportError, TransportResult};

/// HTTP transport scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpScheme {
    /// Plain HTTP.
    Http,
    /// HTTPS with platform trust-root validation.
    Https,
}

/// Parsed HTTP endpoint and joined request path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpTarget {
    scheme: HttpScheme,
    host: String,
    port: u16,
    path: String,
}

impl HttpTarget {
    /// Parses `base_url` and joins its path with `request_path`.
    pub fn parse(base_url: &str, request_path: &str) -> TransportResult<Self> {
        let (scheme, base) = parse_scheme(base_url)?;
        let (authority, base_path) = match base.split_once('/') {
            Some((authority, path)) => (authority, format!("/{path}")),
            None => (base, String::new()),
        };
        let (host, port) = parse_authority(authority, scheme)?;
        let path = join_paths(&base_path, request_path);
        if host.is_empty()
            || host.chars().any(|character| character.is_control())
            || !path.starts_with('/')
            || path.chars().any(|character| character.is_control())
        {
            return Err(TransportError::InvalidRequest(
                "invalid HTTP target".to_string(),
            ));
        }
        Ok(Self {
            scheme,
            host,
            port,
            path,
        })
    }

    /// Returns the transport scheme.
    pub fn scheme(&self) -> HttpScheme {
        self.scheme
    }

    /// Returns the DNS name or IP literal.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Returns the TCP port.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Returns the normalized request path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns an RFC-style authority for the Host header.
    pub fn authority(&self) -> String {
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        if self.port == default_port(self.scheme) {
            host
        } else {
            format!("{host}:{}", self.port)
        }
    }
}

fn parse_scheme(base_url: &str) -> TransportResult<(HttpScheme, &str)> {
    if let Some(base) = base_url.strip_prefix("http://") {
        return Ok((HttpScheme::Http, base));
    }
    if let Some(base) = base_url.strip_prefix("https://") {
        return Ok((HttpScheme::Https, base));
    }
    Err(TransportError::InvalidRequest(
        "URL must use http:// or https://".to_string(),
    ))
}

fn parse_authority(authority: &str, scheme: HttpScheme) -> TransportResult<(String, u16)> {
    if let Some(rest) = authority.strip_prefix('[') {
        let (host, suffix) = rest
            .split_once(']')
            .ok_or_else(|| TransportError::InvalidRequest("invalid IPv6 authority".to_string()))?;
        let port = match suffix.strip_prefix(':') {
            Some(value) => parse_port(value)?,
            None if suffix.is_empty() => default_port(scheme),
            None => {
                return Err(TransportError::InvalidRequest(
                    "invalid IPv6 authority".to_string(),
                ))
            }
        };
        return Ok((host.to_string(), port));
    }
    if authority.matches(':').count() > 1 {
        return Err(TransportError::InvalidRequest(
            "IPv6 hosts require brackets".to_string(),
        ));
    }
    match authority.rsplit_once(':') {
        Some((host, port)) => Ok((host.to_string(), parse_port(port)?)),
        None => Ok((authority.to_string(), default_port(scheme))),
    }
}

fn parse_port(value: &str) -> TransportResult<u16> {
    value
        .parse::<u16>()
        .map_err(|_| TransportError::InvalidRequest("invalid port".to_string()))
}

fn default_port(scheme: HttpScheme) -> u16 {
    match scheme {
        HttpScheme::Http => 80,
        HttpScheme::Https => 443,
    }
}

fn join_paths(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    let path = path.trim_start_matches('/');
    if base.is_empty() {
        format!("/{path}")
    } else if path.is_empty() {
        base.to_string()
    } else {
        format!("{base}/{path}")
    }
}
