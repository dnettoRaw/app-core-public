// =============================================================================
//        #######
//     ###       ###     F: client.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Defines bounded client contracts and behavior for this crate.

use super::*;
use crate::worker::ControlPlaneWorker;

/// Retry limits and exponential backoff bounds for control-plane requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Maximum number of attempts, including the initial request.
    pub max_attempts: usize,
    /// First retry ceiling, capped by `max_backoff_ms`; equal jitter samples
    /// between its rounded-up half and the ceiling before deadline clamping.
    pub initial_backoff_ms: u64,
    /// Maximum exponential delay ceiling between attempts, before equal jitter.
    pub max_backoff_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff_ms: 50,
            max_backoff_ms: 500,
        }
    }
}

/// Configuration for the generic HTTP control-plane client.
/// Execution rejects more than 16 attempts, timeouts outside 1..=30,000 ms,
/// backoffs above 30,000 ms or a conservative retry cycle above 120 seconds.
/// The budget begins at each provider call and includes queueing and encode/decode.
/// Expiry is checked by the worker, not an independent future timer. Transports
/// must cooperate; a late result can mean an operation already applied remotely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlPlaneHttpConfig {
    /// Base URL that hosts the stable control-plane endpoints.
    pub base_url: String,
    /// Per-attempt network timeout.
    pub timeout_ms: u64,
    /// Retry behavior for transport failures and HTTP 408/429/500/502/503/504.
    /// Other HTTP failures and typed semantic rejections return immediately.
    /// Lease mutations always use one attempt because remote deduplication is
    /// not guaranteed; timeout may follow a mutation already applied remotely.
    pub retry_policy: RetryPolicy,
}

/// Transport request produced by [`HttpControlPlaneClient`].
#[derive(Clone, PartialEq, Eq)]
pub struct HttpControlPlaneRequest {
    /// HTTP method.
    pub method: String,
    /// Stable endpoint path relative to the configured base URL.
    pub path: String,
    /// Serialized JSON body.
    pub body: Vec<u8>,
    /// Per-attempt timeout.
    pub timeout_ms: u64,
}

impl std::fmt::Debug for HttpControlPlaneRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HttpControlPlaneRequest")
            .field("method", &self.method)
            .field("path", &self.path)
            .field("body_bytes", &self.body.len())
            .field("timeout_ms", &self.timeout_ms)
            .finish()
    }
}

impl HttpControlPlaneRequest {
    /// Converts this owned request into an immutable request shared across retries.
    pub fn into_shared(self) -> SharedHttpControlPlaneRequest {
        SharedHttpControlPlaneRequest {
            method: self.method,
            path: self.path,
            body: Arc::from(self.body),
            timeout_ms: self.timeout_ms,
        }
    }
}

/// Immutable HTTP request payload that can be reused across bounded retries.
#[derive(Clone, PartialEq, Eq)]
pub struct SharedHttpControlPlaneRequest {
    method: String,
    path: String,
    body: Arc<[u8]>,
    timeout_ms: u64,
}

impl std::fmt::Debug for SharedHttpControlPlaneRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SharedHttpControlPlaneRequest")
            .field("method", &self.method)
            .field("path", &self.path)
            .field("body_bytes", &self.body.len())
            .field("timeout_ms", &self.timeout_ms)
            .finish()
    }
}

impl SharedHttpControlPlaneRequest {
    /// Returns the validated HTTP method candidate.
    pub fn method(&self) -> &str {
        &self.method
    }

    /// Returns the stable endpoint path relative to the configured base URL.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the immutable serialized JSON body.
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    /// Returns the shared serialized JSON body owner.
    pub fn shared_body(&self) -> &Arc<[u8]> {
        &self.body
    }

    /// Returns the per-attempt timeout in milliseconds.
    pub fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }

    fn to_owned(&self) -> HttpControlPlaneRequest {
        HttpControlPlaneRequest {
            method: self.method.clone(),
            path: self.path.clone(),
            body: self.body.to_vec(),
            timeout_ms: self.timeout_ms,
        }
    }
}

/// Bounded response returned by an [`HttpTransport`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpControlPlaneResponse {
    /// HTTP status code.
    pub status_code: u16,
    /// Raw response body.
    pub body: Vec<u8>,
}

/// Adapter contract used by the control-plane HTTP client.
pub trait HttpTransport: Send + Sync {
    /// Sends one JSON request to the configured base URL.
    fn send_json(
        &self,
        base_url: &str,
        request: HttpControlPlaneRequest,
    ) -> ControlPlaneResult<HttpControlPlaneResponse>;

    /// Sends one JSON request and propagates optional trace headers.
    fn send_json_traced(
        &self,
        base_url: &str,
        request: HttpControlPlaneRequest,
        _trace: Option<&TraceContext>,
    ) -> ControlPlaneResult<HttpControlPlaneResponse> {
        self.send_json(base_url, request)
    }

    /// Sends one traced request with cooperative cancellation.
    fn send_json_traced_cancellable(
        &self,
        base_url: &str,
        request: HttpControlPlaneRequest,
        trace: Option<&TraceContext>,
        cancellation: &CancellationToken,
    ) -> ControlPlaneResult<HttpControlPlaneResponse> {
        if cancellation.is_cancelled() {
            return Err(ControlPlaneError::Transport(
                "control-plane request cancelled".to_string(),
            ));
        }
        self.send_json_traced(base_url, request, trace)
    }

    /// Sends a borrowed immutable request that may be shared across retries.
    ///
    /// The compatibility default materializes the original owned request for
    /// transports that have not adopted shared payloads. Built-in transports
    /// override this method and retain only shared body handles.
    fn send_json_shared_traced_cancellable(
        &self,
        base_url: &str,
        request: &SharedHttpControlPlaneRequest,
        trace: Option<&TraceContext>,
        cancellation: &CancellationToken,
    ) -> ControlPlaneResult<HttpControlPlaneResponse> {
        self.send_json_traced_cancellable(base_url, request.to_owned(), trace, cancellation)
    }
}

/// Control-plane client that maps stable contracts onto an HTTP transport.
#[derive(Debug)]
pub struct HttpControlPlaneClient<T> {
    config: ControlPlaneHttpConfig,
    transport: Arc<T>,
    worker: ControlPlaneWorker,
    cancellation: CancellationToken,
    request_started: std::time::Instant,
}

impl<T> Clone for HttpControlPlaneClient<T> {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            transport: Arc::clone(&self.transport),
            worker: self.worker.clone(),
            cancellation: self.cancellation.clone(),
            request_started: self.request_started,
        }
    }
}

impl<T> HttpControlPlaneClient<T>
where
    T: HttpTransport,
{
    /// Creates an HTTP client with a dedicated bounded worker queue.
    /// Dropping a queued request future before dispatch prevents its operation;
    /// dropping after dispatch cannot undo remote effects or preempt transport.
    pub fn new(config: ControlPlaneHttpConfig, transport: T) -> Self {
        Self {
            config,
            transport: Arc::new(transport),
            worker: ControlPlaneWorker::new(),
            cancellation: CancellationToken::new(),
            request_started: std::time::Instant::now(),
        }
    }

    /// Replaces the shared cancellation token used by requests and retries.
    pub fn with_cancellation_token(mut self, cancellation: CancellationToken) -> Self {
        self.cancellation = cancellation;
        self
    }

    /// Cancels queued requests, active official transport I/O, and retry waits.
    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    /// Reports whether this client has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    fn post<Req, Resp>(&self, path: &str, value: &Req) -> ControlPlaneResult<Resp>
    where
        Req: Serialize,
        Resp: for<'de> Deserialize<'de>,
    {
        self.post_traced(path, value, None)
    }

    fn for_request(&self) -> Self {
        let started = std::time::Instant::now();
        Self {
            request_started: started,
            ..self.clone()
        }
    }

    fn post_traced<Req, Resp>(
        &self,
        path: &str,
        value: &Req,
        trace: Option<&TraceContext>,
    ) -> ControlPlaneResult<Resp>
    where
        Req: Serialize,
        Resp: for<'de> Deserialize<'de>,
    {
        let budget = crate::retry_budget::RetryBudget::new_at(&self.config, self.request_started)?;
        budget.remaining()?;
        let body = serde_json::to_vec(value)
            .map_err(|error| ControlPlaneError::Transport(error.to_string()))?;
        let request = HttpControlPlaneRequest {
            method: "POST".to_string(),
            path: path.to_string(),
            body,
            timeout_ms: self.config.timeout_ms,
        };
        self.send_with_retry::<Resp>(request, trace, budget)
    }

    fn send_with_retry<Resp>(
        &self,
        request: HttpControlPlaneRequest,
        trace: Option<&TraceContext>,
        mut budget: crate::retry_budget::RetryBudget,
    ) -> ControlPlaneResult<Resp>
    where
        Resp: for<'de> Deserialize<'de>,
    {
        let mut request = request.into_shared();
        let attempts = if matches!(
            request.path(),
            CONTROL_SERVICE_LEASE_PATH | CONTROL_SERVICE_LEASE_RELEASE_PATH
        ) {
            1 // No remote deduplication contract proves lease mutations safe to replay.
        } else {
            self.config.retry_policy.max_attempts.max(1)
        };
        let mut backoff_ms = self
            .config
            .retry_policy
            .initial_backoff_ms
            .min(self.config.retry_policy.max_backoff_ms);
        let mut last_error = ControlPlaneError::Offline;
        for attempt in 0..attempts {
            request.timeout_ms = budget.attempt_timeout_ms(self.config.timeout_ms)?;
            let response = self.transport.send_json_shared_traced_cancellable(
                &self.config.base_url,
                &request,
                trace,
                &self.cancellation,
            );
            budget.remaining()?;
            match response {
                Ok(response) if (200..300).contains(&response.status_code) => {
                    let result = serde_json::from_slice(&response.body)
                        .map_err(|error| ControlPlaneError::InvalidResponse(error.to_string()));
                    budget.remaining()?;
                    return result;
                }
                Ok(response) => {
                    last_error = ControlPlaneError::Rejected(format!(
                        "http_status={}",
                        response.status_code
                    ));
                    if !matches!(response.status_code, 408 | 429 | 500 | 502 | 503 | 504) {
                        return Err(last_error);
                    }
                }
                Err(
                    error @ (ControlPlaneError::Timeout
                    | ControlPlaneError::Transport(_)
                    | ControlPlaneError::Offline),
                ) => last_error = error,
                Err(error) => return Err(error),
            }

            if attempt + 1 < attempts {
                if self
                    .cancellation
                    .wait_timeout(budget.retry_delay(backoff_ms)?)
                {
                    return Err(ControlPlaneError::Transport(
                        "control-plane request cancelled".to_string(),
                    ));
                }
                backoff_ms =
                    (backoff_ms.saturating_mul(2)).min(self.config.retry_policy.max_backoff_ms);
            }
        }
        Err(last_error)
    }
}

#[path = "client_provider.rs"]
mod provider;
