// =============================================================================
//        #######
//     ###       ###     F: openai_transport.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{AiError, AiResult, AiSecretReference, BackendId, CancellationToken};
use appcore_transport::{HttpClientConfig, HttpHeader, HttpRequest, HttpTarget};
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

/// Boxed asynchronous OpenAI-compatible transport operation.
pub type OpenAiTransportFuture<'a> =
    Pin<Box<dyn Future<Output = AiResult<OpenAiTransportResponse>> + Send + 'a>>;

/// Sequential bounded response-body consumer used by streaming transports.
pub trait OpenAiTransportChunkSink: Send {
    /// Supplies the next bytes from a successful streaming response.
    fn chunk(&mut self, bytes: &[u8]) -> AiResult<()>;
}

/// Secret-free transport request for an OpenAI-compatible server.
#[derive(Clone)]
pub struct OpenAiTransportRequest {
    backend: BackendId,
    base_url: String,
    path: String,
    body: Vec<u8>,
    timeout: Duration,
    max_response_bytes: usize,
    credential: Option<AiSecretReference>,
}

impl OpenAiTransportRequest {
    /// Backend requesting the exchange.
    pub fn backend(&self) -> &BackendId {
        &self.backend
    }

    /// Explicit endpoint base URL. Treat this as deployment-sensitive data.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Validated relative request path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Encoded request bytes. They can contain private prompt content.
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    /// Per-exchange timeout.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Maximum decoded response bytes.
    pub fn max_response_bytes(&self) -> usize {
        self.max_response_bytes
    }

    /// Unresolved AppCore secret reference, never secret material.
    pub fn credential(&self) -> Option<&AiSecretReference> {
        self.credential.as_ref()
    }

    pub(crate) fn new(
        backend: BackendId,
        base_url: String,
        path: String,
        body: Vec<u8>,
        timeout: Duration,
        max_response_bytes: usize,
        credential: Option<AiSecretReference>,
    ) -> Self {
        Self {
            backend,
            base_url,
            path,
            body,
            timeout,
            max_response_bytes,
            credential,
        }
    }
}

impl Debug for OpenAiTransportRequest {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenAiTransportRequest")
            .field("backend", &self.backend)
            .field("endpoint", &"REDACTED")
            .field("path", &self.path)
            .field("redacted_body_bytes", &self.body.len())
            .field("timeout", &self.timeout)
            .field("max_response_bytes", &self.max_response_bytes)
            .field("credential_reference", &self.credential.is_some())
            .finish()
    }
}

/// Bounded HTTP response returned to the backend codec.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenAiTransportResponse {
    /// HTTP status code.
    pub status_code: u16,
    /// Bounded delta parsed from the safe `Retry-After` response header.
    pub retry_after: Option<Duration>,
    /// Bounded response body bytes.
    pub body: Vec<u8>,
}

/// Composition boundary for local HTTP or AppCore-authenticated remote transport.
pub trait OpenAiCompatibleTransport: Send + Sync {
    /// Executes one bounded exchange without logging request or response bodies.
    fn send<'a>(
        &'a self,
        request: &'a OpenAiTransportRequest,
        cancellation: &'a CancellationToken,
    ) -> OpenAiTransportFuture<'a>;

    /// Executes one streaming exchange and supplies successful body chunks in order.
    ///
    /// Implementations must inspect the HTTP status before delivering chunks.
    /// Provider error bodies must never be delivered to the sink or diagnostics.
    fn send_stream<'a>(
        &'a self,
        _request: &'a OpenAiTransportRequest,
        _cancellation: &'a CancellationToken,
        _sink: &'a mut dyn OpenAiTransportChunkSink,
    ) -> OpenAiTransportFuture<'a> {
        Box::pin(async { Err(AiError::Unsupported("OpenAI transport streaming")) })
    }
}

/// Bounded HTTP transport for unauthenticated local/private endpoints.
///
/// A request carrying a credential reference fails closed. Production remote
/// authentication belongs in a composition adapter backed by AppCore security.
#[derive(Clone, Debug)]
pub struct UnauthenticatedOpenAiHttpTransport {
    gate: Arc<crate::openai_blocking::BlockingGate>,
}

impl UnauthenticatedOpenAiHttpTransport {
    /// Creates a transport with an exact bound on concurrent blocking exchanges.
    pub fn new(max_in_flight: usize) -> AiResult<Self> {
        Ok(Self {
            gate: Arc::new(crate::openai_blocking::BlockingGate::new(max_in_flight)?),
        })
    }
}

impl Default for UnauthenticatedOpenAiHttpTransport {
    fn default() -> Self {
        Self {
            gate: Arc::new(crate::openai_blocking::BlockingGate::default()),
        }
    }
}

impl OpenAiCompatibleTransport for UnauthenticatedOpenAiHttpTransport {
    fn send<'a>(
        &'a self,
        request: &'a OpenAiTransportRequest,
        cancellation: &'a CancellationToken,
    ) -> OpenAiTransportFuture<'a> {
        if cancellation.is_cancelled() {
            return Box::pin(async { Err(AiError::Cancelled) });
        }
        if request.credential.is_some() {
            return Box::pin(async { Err(AiError::Unauthorized) });
        }
        let owned = request.clone();
        crate::openai_blocking::run(
            Arc::clone(&self.gate),
            cancellation.clone(),
            move |transport_cancellation| send_blocking(&owned, &transport_cancellation),
        )
    }
}

fn send_blocking(
    request: &OpenAiTransportRequest,
    cancellation: &appcore_transport::CancellationToken,
) -> AiResult<OpenAiTransportResponse> {
    let target = HttpTarget::parse(&request.base_url, &request.path)
        .map_err(|_| backend_failure(request, "invalid-endpoint"))?;
    let http_request = HttpRequest::new("POST", request.body.clone())
        .and_then(|value| {
            Ok(value
                .with_header(HttpHeader::new("Content-Type", "application/json")?)
                .with_header(HttpHeader::new("Accept", "application/json")?))
        })
        .map_err(|_| backend_failure(request, "invalid-http-request"))?;
    let timeout_ms = u64::try_from(request.timeout.as_millis()).unwrap_or(u64::MAX);
    let response = appcore_transport::send(
        &target,
        &http_request,
        HttpClientConfig {
            timeout_ms,
            max_request_bytes: request.body.len(),
            max_response_bytes: request.max_response_bytes,
            max_header_bytes: 32 * 1_024,
        },
        Some(cancellation),
    )
    .map_err(|error| map_transport_error(request, error))?;
    Ok(OpenAiTransportResponse {
        status_code: response.status_code,
        retry_after: retry_after(&response.headers),
        body: response.body,
    })
}

fn map_transport_error(
    request: &OpenAiTransportRequest,
    error: appcore_transport::TransportError,
) -> AiError {
    use appcore_transport::TransportError;
    match error {
        TransportError::Timeout => AiError::DeadlineExceeded,
        TransportError::Cancelled => AiError::Cancelled,
        TransportError::ResponseTooLarge { .. } => backend_failure(request, "response-too-large"),
        _ => backend_failure(request, "transport"),
    }
}

fn retry_after(headers: &[(String, String)]) -> Option<Duration> {
    headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("retry-after"))
        .and_then(|(_, value)| value.parse::<u64>().ok())
        .filter(|seconds| *seconds <= 86_400)
        .map(Duration::from_secs)
}

fn backend_failure(request: &OpenAiTransportRequest, code: &'static str) -> AiError {
    AiError::BackendFailure {
        backend: request.backend.clone(),
        code,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_after_accepts_only_bounded_delta_seconds() {
        assert_eq!(
            retry_after(&[("retry-after".to_string(), "45".to_string())]),
            Some(Duration::from_secs(45))
        );
        assert_eq!(
            retry_after(&[("retry-after".to_string(), "86401".to_string())]),
            None
        );
        assert_eq!(
            retry_after(&[(
                "retry-after".to_string(),
                "Wed, 21 Oct 2015 07:28:00 GMT".to_string()
            )]),
            None
        );
    }
}
