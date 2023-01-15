// =============================================================================
//        #######
//     ###       ###     F: types.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:48:56 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use std::fmt::{Debug, Formatter};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// Result returned by bounded transport operations.
pub type TransportResult<T> = Result<T, TransportError>;

/// Stable low-level transport failure categories.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransportError {
    /// URL, method, path, or headers are invalid.
    #[error("invalid transport request: {0}")]
    InvalidRequest(String),
    /// DNS resolution failed or returned no addresses.
    #[error("DNS resolution failed: {0}")]
    Dns(String),
    /// The remote endpoint refused the TCP connection.
    #[error("connection refused")]
    ConnectionRefused,
    /// Connect, read, or write exceeded the configured deadline.
    #[error("transport timed out")]
    Timeout,
    /// The caller cancelled the operation.
    #[error("transport cancelled")]
    Cancelled,
    /// TLS setup, certificate validation, or negotiation failed.
    #[error("TLS transport failed: {0}")]
    Tls(String),
    /// The request body exceeded its configured limit.
    #[error("request body exceeds {max} bytes")]
    RequestTooLarge {
        /// Maximum accepted request body bytes.
        max: usize,
    },
    /// The response body exceeded its configured limit.
    #[error("response body exceeds {max} bytes")]
    ResponseTooLarge {
        /// Maximum accepted response body bytes.
        max: usize,
    },
    /// The HTTP response is malformed.
    #[error("invalid HTTP response: {0}")]
    InvalidResponse(String),
    /// The peer closed a fixed-size or chunked body before completion.
    #[error("truncated HTTP response")]
    TruncatedResponse,
    /// An operating-system I/O operation failed.
    #[error("transport I/O failed: {0}")]
    Io(String),
}

/// Cloneable cooperative cancellation flag.
#[derive(Debug, Default)]
struct CancellationState {
    cancelled: AtomicBool,
    wait_lock: Mutex<()>,
    wait_signal: Condvar,
}

/// Cloneable cooperative cancellation flag.
#[derive(Debug, Clone, Default)]
pub struct CancellationToken {
    state: Arc<CancellationState>,
}

impl CancellationToken {
    /// Creates a non-cancelled token.
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation.
    pub fn cancel(&self) {
        self.state.cancelled.store(true, Ordering::SeqCst);
        self.state.wait_signal.notify_all();
    }

    /// Reports whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.state.cancelled.load(Ordering::SeqCst)
    }

    /// Waits for cancellation or until `timeout` elapses.
    ///
    /// Returns `true` when cancellation was requested.
    pub fn wait_timeout(&self, timeout: Duration) -> bool {
        if self.is_cancelled() {
            return true;
        }
        let guard = self
            .state
            .wait_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = self
            .state
            .wait_signal
            .wait_timeout_while(guard, timeout, |_| !self.is_cancelled())
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.is_cancelled()
    }
}

impl PartialEq for CancellationToken {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.state, &other.state)
    }
}

impl Eq for CancellationToken {}

/// Validated HTTP header name and value.
#[derive(Clone, PartialEq, Eq)]
pub struct HttpHeader {
    name: String,
    value: String,
    sensitive: bool,
}

impl HttpHeader {
    /// Creates a non-sensitive header.
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> TransportResult<Self> {
        Self::build(name.into(), value.into(), false)
    }

    /// Creates a header whose value is always redacted from `Debug`.
    pub fn sensitive(name: impl Into<String>, value: impl Into<String>) -> TransportResult<Self> {
        Self::build(name.into(), value.into(), true)
    }

    /// Returns the header name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the header value for request serialization.
    pub fn value(&self) -> &str {
        &self.value
    }

    fn build(name: String, value: String, sensitive: bool) -> TransportResult<Self> {
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            || value.chars().any(|character| character.is_control())
        {
            return Err(TransportError::InvalidRequest(
                "invalid HTTP header".to_string(),
            ));
        }
        Ok(Self {
            name,
            value,
            sensitive,
        })
    }
}

impl Debug for HttpHeader {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HttpHeader")
            .field("name", &self.name)
            .field(
                "value",
                if self.sensitive || is_sensitive_header(&self.name) {
                    &"REDACTED"
                } else {
                    &self.value
                },
            )
            .finish()
    }
}

fn is_sensitive_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "authorization"
            | "proxy-authorization"
            | "cookie"
            | "set-cookie"
            | "x-api-key"
            | "x-auth-token"
    )
}

/// HTTP request bytes plus transport-neutral headers.
#[derive(Clone, PartialEq, Eq)]
pub struct HttpRequest {
    method: String,
    headers: Vec<HttpHeader>,
    body: Vec<u8>,
}

impl Debug for HttpRequest {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HttpRequest")
            .field("method", &self.method)
            .field("headers", &self.headers)
            .field("body_bytes", &self.body.len())
            .finish()
    }
}

impl HttpRequest {
    /// Creates a request with an uppercase method and body bytes.
    pub fn new(method: impl Into<String>, body: Vec<u8>) -> TransportResult<Self> {
        let method = method.into();
        if method.is_empty() || !method.bytes().all(|byte| byte.is_ascii_uppercase()) {
            return Err(TransportError::InvalidRequest(
                "HTTP method must use uppercase ASCII".to_string(),
            ));
        }
        Ok(Self {
            method,
            headers: Vec::new(),
            body,
        })
    }

    /// Appends a validated header.
    pub fn with_header(mut self, header: HttpHeader) -> Self {
        self.headers.push(header);
        self
    }

    /// Returns the HTTP method.
    pub fn method(&self) -> &str {
        &self.method
    }

    /// Returns request headers.
    pub fn headers(&self) -> &[HttpHeader] {
        &self.headers
    }

    /// Returns request body bytes.
    pub fn body(&self) -> &[u8] {
        &self.body
    }
}

/// Resource limits and deadline for one blocking exchange.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HttpClientConfig {
    /// Connect, read, and write deadline in milliseconds.
    pub timeout_ms: u64,
    /// Maximum request body size in bytes.
    pub max_request_bytes: usize,
    /// Maximum decoded response body size in bytes.
    pub max_response_bytes: usize,
    /// Maximum response header size in bytes.
    pub max_header_bytes: usize,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            timeout_ms: 5_000,
            max_request_bytes: 1_048_576,
            max_response_bytes: 1_048_576,
            max_header_bytes: 32_768,
        }
    }
}

/// Parsed HTTP response with lowercase header names.
#[derive(Clone, PartialEq, Eq)]
pub struct HttpResponse {
    /// Numeric HTTP status code.
    pub status_code: u16,
    /// Response headers in received order.
    pub headers: Vec<(String, String)>,
    /// Decoded response body.
    pub body: Vec<u8>,
}

impl Debug for HttpResponse {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HttpResponse")
            .field("status_code", &self.status_code)
            .field("header_count", &self.headers.len())
            .field("body_bytes", &self.body.len())
            .finish()
    }
}
