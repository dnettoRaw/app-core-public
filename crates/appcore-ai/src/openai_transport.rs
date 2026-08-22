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
use std::time::Duration;

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
    /// Bounded response body bytes.
    pub body: Vec<u8>,
}

/// Composition boundary for local HTTP or AppCore-authenticated remote transport.
pub trait OpenAiCompatibleTransport: Send + Sync {
    /// Executes one bounded exchange without logging request or response bodies.
    fn send(
        &self,
        request: &OpenAiTransportRequest,
        cancellation: &CancellationToken,
    ) -> AiResult<OpenAiTransportResponse>;
}

/// Bounded HTTP transport for unauthenticated local/private endpoints.
///
/// A request carrying a credential reference fails closed. Production remote
/// authentication belongs in a composition adapter backed by AppCore security.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnauthenticatedOpenAiHttpTransport;

impl OpenAiCompatibleTransport for UnauthenticatedOpenAiHttpTransport {
    fn send(
        &self,
        request: &OpenAiTransportRequest,
        cancellation: &CancellationToken,
    ) -> AiResult<OpenAiTransportResponse> {
        if cancellation.is_cancelled() {
            return Err(AiError::Cancelled);
        }
        if request.credential.is_some() {
            return Err(AiError::Unauthorized);
        }
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
            None,
        )
        .map_err(|error| {
            use appcore_transport::TransportError;
            match error {
                TransportError::Timeout => AiError::DeadlineExceeded,
                TransportError::Cancelled => AiError::Cancelled,
                TransportError::ResponseTooLarge { .. } => {
                    backend_failure(request, "response-too-large")
                }
                _ => backend_failure(request, "transport"),
            }
        })?;
        if cancellation.is_cancelled() {
            return Err(AiError::Cancelled);
        }
        Ok(OpenAiTransportResponse {
            status_code: response.status_code,
            body: response.body,
        })
    }
}

fn backend_failure(request: &OpenAiTransportRequest, code: &'static str) -> AiError {
    AiError::BackendFailure {
        backend: request.backend.clone(),
        code,
    }
}
