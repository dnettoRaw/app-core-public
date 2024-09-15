// =============================================================================
//        #######
//     ###       ###     F: transport.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/02 13:08:16 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Transport contracts and the manual HTTP sync transport.

use crate::sync::discovery::SyncPeerScheme;
use crate::sync::error::{SyncError, SyncResult};
use crate::sync::types::{HeartbeatMessage, PeerInfo, SyncMessage};
use appcore_core::CoreIdentity;
use appcore_transport::{
    send, CancellationToken, HttpClientConfig, HttpHeader, HttpRequest, HttpTarget, TransportError,
};
use std::fmt;
#[cfg(test)]
use std::io::Read;
#[cfg(test)]
use std::net::TcpStream;

const DEFAULT_TIMEOUT_MS: u64 = 5_000;
const DEFAULT_MAX_RESPONSE_BYTES: usize = 64 * 1024;
const DEFAULT_MAX_REQUEST_BODY_BYTES: usize = 1024 * 1024;

/// Sync transport contract.
pub trait SyncTransport {
    /// Sends an operational heartbeat.
    fn send_heartbeat(&mut self, heartbeat: HeartbeatMessage) -> SyncResult<()>;
    /// Sends opaque payload bytes to a compatible peer.
    fn send_payload(&mut self, peer: &PeerInfo, payload: Vec<u8>) -> SyncResult<()>;
}

// Plain HTTP does not replace TLS/mTLS. The v1 body binds the source identity,
// but transport confidentiality and server authentication still require HTTPS
// or an external secure tunnel.
//
// O parsing HTTP é feito de forma manual para manter o runtime minimalista e sem dependências pesadas,
// mas exige limites rígidos de timeouts e tamanho de payload para evitar DoS por peers maliciosos ou lentos.
#[derive(Clone, PartialEq, Eq)]
/// Bounded blocking HTTP client for leader-to-follower sync batches.
pub struct HttpSyncTransport {
    host: String,
    port: u16,
    scheme: SyncPeerScheme,
    auth_token: Option<String>,
    timeout_ms: u64,
    max_response_bytes: usize,
    max_request_body_bytes: usize,
    source_identity: Option<CoreIdentity>,
    cancellation: CancellationToken,
}

impl fmt::Debug for HttpSyncTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpSyncTransport")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("scheme", &self.scheme)
            .field("auth_configured", &self.auth_token.is_some())
            .field("timeout_ms", &self.timeout_ms)
            .field("max_response_bytes", &self.max_response_bytes)
            .field("max_request_body_bytes", &self.max_request_body_bytes)
            .field("source_identity", &self.source_identity)
            .field("cancelled", &self.cancellation.is_cancelled())
            .finish()
    }
}

impl HttpSyncTransport {
    /// Creates a plain-HTTP transport for `host` and `port`.
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            scheme: SyncPeerScheme::Http,
            auth_token: None,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
            max_request_body_bytes: DEFAULT_MAX_REQUEST_BODY_BYTES,
            source_identity: None,
            cancellation: CancellationToken::new(),
        }
    }

    /// Adds a bearer token without exposing it through `Debug`.
    pub fn with_auth_token(mut self, token: impl Into<String>) -> Self {
        self.auth_token = Some(token.into());
        self
    }

    /// Selects the HTTP or HTTPS transport scheme.
    pub fn with_scheme(mut self, scheme: SyncPeerScheme) -> Self {
        self.scheme = scheme;
        self
    }

    /// Selects HTTPS transport.
    pub fn with_https(mut self) -> Self {
        self.scheme = SyncPeerScheme::Https;
        self
    }

    /// Sets connect, read, and write deadlines in milliseconds.
    pub fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// Sets the maximum number of response bytes read from a peer.
    pub fn with_max_response_bytes(mut self, max_response_bytes: usize) -> Self {
        self.max_response_bytes = max_response_bytes;
        self
    }

    /// Sets the maximum encoded request-body size.
    pub fn with_max_request_body_bytes(mut self, max_request_body_bytes: usize) -> Self {
        self.max_request_body_bytes = max_request_body_bytes;
        self
    }

    /// Uses the identity-aware `appcore.sync.v1` envelope for outbound batches.
    pub fn with_source_identity(mut self, source_identity: CoreIdentity) -> Self {
        self.source_identity = Some(source_identity);
        self
    }

    /// Replaces the cooperative cancellation token used by transport I/O.
    pub fn with_cancellation_token(mut self, cancellation: CancellationToken) -> Self {
        self.cancellation = cancellation;
        self
    }

    /// Cancels active and future transport operations.
    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    /// Reports whether cancellation was requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    pub(crate) fn cancellation_token(&self) -> &CancellationToken {
        &self.cancellation
    }

    /// Posts a batch to `/v1/sync/events` and requires a 2xx response.
    pub fn post_sync_events(&self, message: &SyncMessage) -> SyncResult<()> {
        let identity = self
            .source_identity
            .as_ref()
            .ok_or(SyncError::InvalidSyncMessage(
                "local sync identity is not configured",
            ))?;
        let body = crate::sync::wire::encode_sync_envelope_v1(identity, message)?;
        if body.len() > self.max_request_body_bytes {
            return Err(SyncError::RequestBodyTooLarge {
                size: body.len(),
                max: self.max_request_body_bytes,
            });
        }
        let target =
            HttpTarget::parse(&self.base_url(), "/v1/sync/events").map_err(map_transport_error)?;
        let mut request = HttpRequest::new("POST", body.into_bytes())
            .map_err(map_transport_error)?
            .with_header(
                HttpHeader::new("Content-Type", "application/json").map_err(map_transport_error)?,
            );
        if let Some(token) = &self.auth_token {
            request = request.with_header(
                HttpHeader::sensitive("Authorization", format!("Bearer {token}"))
                    .map_err(map_transport_error)?,
            );
        }
        let response = send(
            &target,
            &request,
            HttpClientConfig {
                timeout_ms: self.timeout_ms,
                max_request_bytes: self.max_request_body_bytes,
                max_response_bytes: self.max_response_bytes,
                max_header_bytes: self.max_response_bytes,
            },
            Some(&self.cancellation),
        )
        .map_err(map_transport_error)?;
        if (200..300).contains(&response.status_code) {
            return Ok(());
        }
        Err(SyncError::HttpStatus(response.status_code))
    }

    fn base_url(&self) -> String {
        format!("{}://{}:{}", self.scheme.as_str(), self.host, self.port)
    }
}

fn map_transport_error(error: TransportError) -> SyncError {
    match error {
        TransportError::Timeout => SyncError::TransportTimeout("read".to_string()),
        TransportError::Dns(reason) => SyncError::DnsResolutionFailed(reason),
        TransportError::Tls(reason) => SyncError::TlsFailed(reason),
        TransportError::Cancelled => {
            SyncError::TransportFailed("sync transport cancelled".to_string())
        }
        TransportError::ResponseTooLarge { max } => SyncError::ResponseTooLarge { max },
        TransportError::RequestTooLarge { max } => SyncError::RequestBodyTooLarge {
            size: max.saturating_add(1),
            max,
        },
        TransportError::InvalidResponse(reason) if reason == "empty response" => {
            SyncError::EmptyHttpResponse
        }
        other => SyncError::TransportFailed(other.to_string()),
    }
}

/// Decodes a versioned v1 envelope and returns its replication message.
pub fn decode_sync_message(input: &str) -> SyncResult<SyncMessage> {
    crate::sync::wire::decode_sync_envelope(input).map(|envelope| envelope.message)
}

#[cfg(test)]
pub(crate) fn read_http_request_body(stream: &mut TcpStream) -> SyncResult<String> {
    let mut buffer = Vec::new();
    let mut headers_end = None;
    let mut content_length = 0usize;
    loop {
        let mut chunk = [0u8; 512];
        let read = stream
            .read(&mut chunk)
            .map_err(|err| SyncError::TransportFailed(err.to_string()))?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if headers_end.is_none() {
            headers_end = find_headers_end(&buffer);
            if let Some(end) = headers_end {
                content_length = parse_content_length(&buffer[..end])?;
            }
        }
        if let Some(end) = headers_end {
            let body_len = buffer.len().saturating_sub(end);
            if body_len >= content_length {
                let body = &buffer[end..end + content_length];
                return String::from_utf8(body.to_vec())
                    .map_err(|_| SyncError::TransportFailed("invalid request body".to_string()));
            }
        }
    }
    Err(SyncError::TransportFailed(
        "incomplete HTTP request".to_string(),
    ))
}

#[cfg(test)]
fn find_headers_end(buffer: &[u8]) -> Option<usize> {
    let marker = b"\r\n\r\n";
    buffer
        .windows(marker.len())
        .position(|window| window == marker)
        .map(|position| position + marker.len())
}

#[cfg(test)]
fn parse_content_length(headers: &[u8]) -> SyncResult<usize> {
    let as_text = String::from_utf8(headers.to_vec())
        .map_err(|_| SyncError::TransportFailed("invalid HTTP headers".to_string()))?;
    for line in as_text.lines() {
        if let Some(value) = line.strip_prefix("Content-Length:") {
            return value
                .trim()
                .parse::<usize>()
                .map_err(|_| SyncError::TransportFailed("invalid content length".to_string()));
        }
    }
    Err(SyncError::TransportFailed(
        "missing content length".to_string(),
    ))
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;
