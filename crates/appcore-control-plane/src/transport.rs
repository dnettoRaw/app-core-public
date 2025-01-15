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
#[cfg(test)]
use appcore_transport::parse_response;
use appcore_transport::{
    send, HttpClientConfig, HttpHeader, HttpRequest, HttpScheme as SharedHttpScheme,
    HttpTarget as SharedHttpTarget, TransportError,
};
use zeroize::Zeroizing;

/// Shared bounded HTTP transport for unauthenticated deployment-local calls.
#[derive(Debug, Clone, Copy)]
pub struct StdHttpTransport;

/// Rejects plain HTTP for non-loopback control-plane endpoints.
pub fn require_secure_remote_endpoint(endpoint: &str) -> ControlPlaneResult<()> {
    let target = SharedHttpTarget::parse(endpoint, "/")
        .map_err(|error| ControlPlaneError::Rejected(format!("invalid endpoint: {error}")))?;
    if target.scheme() == SharedHttpScheme::Https
        || matches!(target.host(), "127.0.0.1" | "::1" | "localhost")
    {
        return Ok(());
    }
    Err(ControlPlaneError::Rejected(
        "remote control-plane endpoint requires HTTPS".to_string(),
    ))
}

/// Redacted bearer material that is zeroized when released.
#[derive(Clone)]
pub struct SecretString(Zeroizing<String>);

impl SecretString {
    /// Wraps an owned secret value.
    pub fn new(value: impl Into<String>) -> Self {
        Self(Zeroizing::new(value.into()))
    }

    /// Adopts an already-zeroizing value without a plain-text copy.
    pub fn from_zeroizing(value: Zeroizing<String>) -> Self {
        Self(value)
    }

    fn expose(&self) -> &str {
        self.0.as_str()
    }
}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SecretString(REDACTED)")
    }
}

/// HTTP transport that authenticates requests with a zeroizing bearer token.
#[derive(Clone)]
pub struct BearerHttpTransport {
    bearer_token: SecretString,
    max_response_bytes: usize,
}

impl BearerHttpTransport {
    /// Creates a bearer transport without exposing or copying its secret.
    pub fn from_secret(bearer_token: SecretString) -> Self {
        Self {
            bearer_token,
            max_response_bytes: DEFAULT_MAX_HTTP_RESPONSE_BYTES,
        }
    }

    /// Sets the maximum accepted response body size in bytes.
    pub fn with_max_response_bytes(mut self, max_response_bytes: usize) -> Self {
        self.max_response_bytes = max_response_bytes.max(1);
        self
    }
}

impl std::fmt::Debug for BearerHttpTransport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BearerHttpTransport")
            .field("bearer_token", &"REDACTED")
            .field("max_response_bytes", &self.max_response_bytes)
            .finish()
    }
}

impl HttpTransport for StdHttpTransport {
    fn send_json(
        &self,
        base_url: &str,
        request: HttpControlPlaneRequest,
    ) -> ControlPlaneResult<HttpControlPlaneResponse> {
        send_http_json(
            base_url,
            request,
            None,
            DEFAULT_MAX_HTTP_RESPONSE_BYTES,
            None,
            None,
        )
    }

    fn send_json_traced(
        &self,
        base_url: &str,
        request: HttpControlPlaneRequest,
        trace: Option<&TraceContext>,
    ) -> ControlPlaneResult<HttpControlPlaneResponse> {
        send_http_json(
            base_url,
            request,
            None,
            DEFAULT_MAX_HTTP_RESPONSE_BYTES,
            trace,
            None,
        )
    }

    fn send_json_traced_cancellable(
        &self,
        base_url: &str,
        request: HttpControlPlaneRequest,
        trace: Option<&TraceContext>,
        cancellation: &CancellationToken,
    ) -> ControlPlaneResult<HttpControlPlaneResponse> {
        send_http_json(
            base_url,
            request,
            None,
            DEFAULT_MAX_HTTP_RESPONSE_BYTES,
            trace,
            Some(cancellation),
        )
    }
}

impl HttpTransport for BearerHttpTransport {
    fn send_json(
        &self,
        base_url: &str,
        request: HttpControlPlaneRequest,
    ) -> ControlPlaneResult<HttpControlPlaneResponse> {
        self.send(base_url, request, None)
    }

    fn send_json_traced(
        &self,
        base_url: &str,
        request: HttpControlPlaneRequest,
        trace: Option<&TraceContext>,
    ) -> ControlPlaneResult<HttpControlPlaneResponse> {
        self.send(base_url, request, trace)
    }

    fn send_json_traced_cancellable(
        &self,
        base_url: &str,
        request: HttpControlPlaneRequest,
        trace: Option<&TraceContext>,
        cancellation: &CancellationToken,
    ) -> ControlPlaneResult<HttpControlPlaneResponse> {
        self.send_cancellable(base_url, request, trace, Some(cancellation))
    }
}

impl BearerHttpTransport {
    fn send(
        &self,
        base_url: &str,
        request: HttpControlPlaneRequest,
        trace: Option<&TraceContext>,
    ) -> ControlPlaneResult<HttpControlPlaneResponse> {
        self.send_cancellable(base_url, request, trace, None)
    }

    fn send_cancellable(
        &self,
        base_url: &str,
        request: HttpControlPlaneRequest,
        trace: Option<&TraceContext>,
        cancellation: Option<&CancellationToken>,
    ) -> ControlPlaneResult<HttpControlPlaneResponse> {
        send_http_json(
            base_url,
            request,
            Some(self.bearer_token.expose()),
            self.max_response_bytes,
            trace,
            cancellation,
        )
    }
}

fn send_http_json(
    base_url: &str,
    request: HttpControlPlaneRequest,
    bearer_token: Option<&str>,
    max_response_bytes: usize,
    trace: Option<&TraceContext>,
    cancellation: Option<&CancellationToken>,
) -> ControlPlaneResult<HttpControlPlaneResponse> {
    validate_http_request(&request, bearer_token)?;
    let target = SharedHttpTarget::parse(base_url, &request.path).map_err(map_transport_error)?;
    let transport_request = build_request(&request, bearer_token, trace)?;
    let response = send(
        &target,
        &transport_request,
        HttpClientConfig {
            timeout_ms: request.timeout_ms.max(1),
            max_request_bytes: DEFAULT_MAX_HTTP_RESPONSE_BYTES,
            max_response_bytes,
            max_header_bytes: MAX_HTTP_HEADER_BYTES,
        },
        cancellation,
    )
    .map_err(map_transport_error)?;
    Ok(HttpControlPlaneResponse {
        status_code: response.status_code,
        body: response.body,
    })
}

fn build_request(
    request: &HttpControlPlaneRequest,
    bearer_token: Option<&str>,
    trace: Option<&TraceContext>,
) -> ControlPlaneResult<HttpRequest> {
    let mut transport = HttpRequest::new(request.method.clone(), request.body.clone())
        .map_err(map_transport_error)?
        .with_header(
            HttpHeader::new("Content-Type", "application/json").map_err(map_transport_error)?,
        )
        .with_header(HttpHeader::new("Accept", "application/json").map_err(map_transport_error)?)
        .with_header(HttpHeader::new("Accept-Encoding", "gzip").map_err(map_transport_error)?);
    if let Some(token) = bearer_token {
        transport = transport.with_header(
            HttpHeader::sensitive("Authorization", format!("Bearer {token}"))
                .map_err(map_transport_error)?,
        );
    }
    for header in trace_http_headers(trace)? {
        transport = transport.with_header(header);
    }
    Ok(transport)
}

fn validate_http_request(
    request: &HttpControlPlaneRequest,
    bearer_token: Option<&str>,
) -> ControlPlaneResult<()> {
    if request.method.is_empty() || !request.method.bytes().all(|byte| byte.is_ascii_uppercase()) {
        return Err(ControlPlaneError::Transport(
            "invalid HTTP method".to_string(),
        ));
    }
    if bearer_token.is_some_and(|token| {
        token.is_empty() || token.chars().any(|character| character.is_control())
    }) {
        return Err(ControlPlaneError::Transport(
            "invalid bearer credential".to_string(),
        ));
    }
    Ok(())
}

fn trace_http_headers(trace: Option<&TraceContext>) -> ControlPlaneResult<Vec<HttpHeader>> {
    let Some(trace) = trace else {
        return Ok(Vec::new());
    };
    let fields = [
        ("X-AppCore-Trace-Id", Some(trace.trace_id.as_str())),
        ("X-AppCore-Span-Id", Some(trace.span_id.as_str())),
        ("X-AppCore-Parent-Span-Id", trace.parent_span_id.as_deref()),
        (
            "X-AppCore-Origin-Core-Id",
            Some(trace.originating_core_id.as_str()),
        ),
        (
            "X-AppCore-Current-Core-Id",
            Some(trace.current_core_id.as_str()),
        ),
        ("X-AppCore-Tenant-Id", Some(trace.tenant_id.as_str())),
        ("X-AppCore-Command-Id", trace.command_id.as_deref()),
    ];
    fields
        .into_iter()
        .filter_map(|(name, value)| value.map(|value| (name, value)))
        .map(|(name, value)| HttpHeader::new(name, value).map_err(map_transport_error))
        .collect()
}

#[cfg(test)]
pub(crate) fn control_plane_trace_headers(
    trace: Option<&TraceContext>,
) -> ControlPlaneResult<String> {
    let mut encoded = String::new();
    for header in trace_http_headers(trace)? {
        encoded.push_str(header.name());
        encoded.push_str(": ");
        encoded.push_str(header.value());
        encoded.push_str("\r\n");
    }
    Ok(encoded)
}

fn map_transport_error(error: TransportError) -> ControlPlaneError {
    match error {
        TransportError::Timeout => ControlPlaneError::Timeout,
        TransportError::InvalidResponse(reason) => ControlPlaneError::InvalidResponse(reason),
        TransportError::TruncatedResponse => {
            ControlPlaneError::InvalidResponse("truncated HTTP response".to_string())
        }
        TransportError::ResponseTooLarge { .. } => {
            ControlPlaneError::InvalidResponse("HTTP response exceeds configured limit".to_string())
        }
        other => ControlPlaneError::Transport(other.to_string()),
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HttpScheme {
    Http,
    Https,
}

#[cfg(test)]
pub(crate) struct HttpTarget {
    pub(crate) scheme: HttpScheme,
    pub(crate) port: u16,
    pub(crate) path: String,
}

#[cfg(test)]
impl HttpTarget {
    pub(crate) fn parse(base_url: &str, path: &str) -> ControlPlaneResult<Self> {
        let target = SharedHttpTarget::parse(base_url, path).map_err(map_transport_error)?;
        Ok(Self {
            scheme: match target.scheme() {
                SharedHttpScheme::Http => HttpScheme::Http,
                SharedHttpScheme::Https => HttpScheme::Https,
            },
            port: target.port(),
            path: target.path().to_string(),
        })
    }
}

#[cfg(test)]
pub(crate) fn parse_http_response(
    raw: &[u8],
    max_response_bytes: usize,
) -> ControlPlaneResult<HttpControlPlaneResponse> {
    let response = parse_response(raw, MAX_HTTP_HEADER_BYTES, max_response_bytes)
        .map_err(map_transport_error)?;
    Ok(HttpControlPlaneResponse {
        status_code: response.status_code,
        body: response.body,
    })
}
