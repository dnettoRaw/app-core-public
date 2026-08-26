// =============================================================================
//        #######
//     ###       ###     F: transport.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::*;
use appcore_transport::{
    decode_gzip_limited, encode_gzip_if_smaller, send, HttpClient, HttpClientConfig, HttpHeader,
    HttpRequest, HttpTarget, TransportError,
};
#[cfg(test)]
use appcore_transport::{parse_response, HttpScheme};

/// Bounded shared HTTP and HTTPS transport for peer RPC clients.
#[derive(Debug, Clone, Copy)]
pub struct StdPeerRpcTransport;

/// Reusable bounded HTTP and HTTPS transport for peer RPC clients.
#[derive(Debug, Clone, Default)]
pub struct PooledPeerRpcTransport {
    client: HttpClient,
}

impl PeerTransportProvider for StdPeerRpcTransport {
    fn send(
        &self,
        base_url: &str,
        request: PeerRpcHttpRequest,
    ) -> Result<PeerRpcHttpResponse, PeerRpcError> {
        self.send_request(base_url, request, None)
    }

    fn send_cancellable(
        &self,
        base_url: &str,
        request: PeerRpcHttpRequest,
        cancellation: &CancellationToken,
    ) -> Result<PeerRpcHttpResponse, PeerRpcError> {
        self.send_request(base_url, request, Some(cancellation))
    }
}

impl StdPeerRpcTransport {
    fn send_request(
        &self,
        base_url: &str,
        request: PeerRpcHttpRequest,
        cancellation: Option<&CancellationToken>,
    ) -> Result<PeerRpcHttpResponse, PeerRpcError> {
        send_request(None, base_url, request, cancellation)
    }
}

impl PeerTransportProvider for PooledPeerRpcTransport {
    fn send(
        &self,
        base_url: &str,
        request: PeerRpcHttpRequest,
    ) -> Result<PeerRpcHttpResponse, PeerRpcError> {
        send_request(Some(&self.client), base_url, request, None)
    }

    fn send_cancellable(
        &self,
        base_url: &str,
        request: PeerRpcHttpRequest,
        cancellation: &CancellationToken,
    ) -> Result<PeerRpcHttpResponse, PeerRpcError> {
        send_request(Some(&self.client), base_url, request, Some(cancellation))
    }
}

fn send_request(
    client: Option<&HttpClient>,
    base_url: &str,
    request: PeerRpcHttpRequest,
    cancellation: Option<&CancellationToken>,
) -> Result<PeerRpcHttpResponse, PeerRpcError> {
    validate_peer_http_request(&request)?;
    let target = HttpTarget::parse(base_url, &request.path).map_err(map_transport_error)?;
    let (body, compressed) = compress_request_body(&request.body)?;
    let transport_request = build_request(&request, body, compressed)?;
    let config = HttpClientConfig {
        timeout_ms: request.timeout_ms.max(1),
        max_request_bytes: request
            .body
            .len()
            .saturating_add(MAX_ENVELOPE_OVERHEAD_BYTES),
        max_response_bytes: request.max_response_bytes,
        max_header_bytes: MAX_HTTP_HEADER_BYTES,
    };
    let response = match client {
        Some(client) => client.send(&target, &transport_request, config.into(), cancellation),
        None => send(&target, &transport_request, config, cancellation),
    }
    .map_err(map_transport_error)?;
    Ok(PeerRpcHttpResponse {
        status_code: response.status_code,
        body: response.body,
    })
}

pub(crate) fn http_status_error(status_code: u16, body: Vec<u8>) -> PeerRpcError {
    match status_code {
        401 => PeerRpcError::Unauthorized,
        403 => PeerRpcError::Forbidden,
        409 => PeerRpcError::ProtocolMismatch,
        413 => PeerRpcError::PayloadTooLarge,
        408 | 429 | 500..=599 => PeerRpcError::EndpointUnavailable,
        _ => {
            let _ = body;
            PeerRpcError::InvalidResponse(format!("http_status={status_code}"))
        }
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PeerHttpScheme {
    Http,
    Https,
}

#[cfg(test)]
pub(crate) struct PeerHttpTarget {
    pub(crate) scheme: PeerHttpScheme,
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) path: String,
}

#[cfg(test)]
impl PeerHttpTarget {
    pub(crate) fn parse(base_url: &str, path: &str) -> Result<Self, PeerRpcError> {
        let target = HttpTarget::parse(base_url, path).map_err(map_transport_error)?;
        Ok(Self {
            scheme: match target.scheme() {
                HttpScheme::Http => PeerHttpScheme::Http,
                HttpScheme::Https => PeerHttpScheme::Https,
            },
            host: target.host().to_string(),
            port: target.port(),
            path: target.path().to_string(),
        })
    }

    pub(crate) fn authority(&self) -> String {
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        let default = match self.scheme {
            PeerHttpScheme::Http => 80,
            PeerHttpScheme::Https => 443,
        };
        if self.port == default {
            host
        } else {
            format!("{host}:{}", self.port)
        }
    }
}

fn build_request(
    request: &PeerRpcHttpRequest,
    body: Vec<u8>,
    compressed: bool,
) -> Result<HttpRequest, PeerRpcError> {
    let mut transport = HttpRequest::new(request.method.clone(), body)
        .map_err(map_transport_error)?
        .with_header(
            HttpHeader::new("Content-Type", "application/json").map_err(map_transport_error)?,
        )
        .with_header(HttpHeader::new("Accept", "application/json").map_err(map_transport_error)?)
        .with_header(HttpHeader::new("Accept-Encoding", "gzip").map_err(map_transport_error)?);
    if compressed {
        transport = transport
            .with_header(HttpHeader::new("Content-Encoding", "gzip").map_err(map_transport_error)?);
    }
    if let Some(token) = &request.bearer_token {
        transport = transport.with_header(
            HttpHeader::sensitive("Authorization", format!("Bearer {token}"))
                .map_err(map_transport_error)?,
        );
    }
    Ok(transport)
}

fn compress_request_body(body: &[u8]) -> Result<(Vec<u8>, bool), PeerRpcError> {
    if body.len() < COMPRESSION_THRESHOLD_BYTES {
        return Ok((body.to_vec(), false));
    }
    match encode_gzip_if_smaller(body).map_err(map_transport_error)? {
        Some(compressed) => Ok((compressed, true)),
        None => Ok((body.to_vec(), false)),
    }
}

fn validate_peer_http_request(request: &PeerRpcHttpRequest) -> Result<(), PeerRpcError> {
    if request.method.is_empty() || !request.method.bytes().all(|byte| byte.is_ascii_uppercase()) {
        return Err(PeerRpcError::Transport("invalid HTTP method".to_string()));
    }
    if request.bearer_token.as_ref().is_some_and(|token| {
        token.is_empty() || token.chars().any(|character| character.is_control())
    }) {
        return Err(PeerRpcError::Transport(
            "invalid bearer credential".to_string(),
        ));
    }
    Ok(())
}

fn map_transport_error(error: TransportError) -> PeerRpcError {
    match error {
        TransportError::ResponseTooLarge { .. } | TransportError::RequestTooLarge { .. } => {
            PeerRpcError::PayloadTooLarge
        }
        TransportError::Timeout
        | TransportError::ConnectionRefused
        | TransportError::Dns(_)
        | TransportError::Cancelled => PeerRpcError::EndpointUnavailable,
        TransportError::InvalidResponse(reason) => PeerRpcError::InvalidResponse(reason),
        TransportError::TruncatedResponse => {
            PeerRpcError::InvalidResponse("truncated HTTP response".to_string())
        }
        other => PeerRpcError::Transport(other.to_string()),
    }
}

#[cfg(test)]
pub(crate) fn parse_http_response(
    raw: &[u8],
    max_response_bytes: usize,
) -> Result<PeerRpcHttpResponse, PeerRpcError> {
    let response = parse_response(raw, MAX_HTTP_HEADER_BYTES, max_response_bytes)
        .map_err(map_transport_error)?;
    Ok(PeerRpcHttpResponse {
        status_code: response.status_code,
        body: response.body,
    })
}

pub(crate) fn gzip_if_beneficial(input: &[u8]) -> Result<Option<Vec<u8>>, PeerRpcError> {
    if input.len() < COMPRESSION_THRESHOLD_BYTES {
        return Ok(None);
    }
    encode_gzip_if_smaller(input).map_err(map_transport_error)
}

pub(crate) fn gzip_decode_limited(input: &[u8], max_bytes: usize) -> Result<Vec<u8>, PeerRpcError> {
    decode_gzip_limited(input, max_bytes).map_err(map_transport_error)
}
