// =============================================================================
//        #######
//     ###       ###     F: client.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:48:56 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::*;
use crate::transport::http_status_error;

/// Retry limits and exponential backoff bounds for peer requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerRpcRetryPolicy {
    /// Maximum attempts, including the initial request.
    pub max_attempts: usize,
    /// Delay before the first retry.
    pub initial_backoff_ms: u64,
    /// Maximum delay between attempts.
    pub max_backoff_ms: u64,
}

impl Default for PeerRpcRetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 2,
            initial_backoff_ms: 50,
            max_backoff_ms: 500,
        }
    }
}

/// Runtime limits used by [`PeerRpcClient`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerRpcClientConfig {
    /// Per-attempt network timeout.
    pub request_timeout_ms: u64,
    /// Lifetime assigned to signed request envelopes.
    pub envelope_ttl_ms: u64,
    /// Maximum accepted response body size.
    pub max_response_bytes: usize,
    /// Retry behavior for transport failures.
    pub retry_policy: PeerRpcRetryPolicy,
}

impl Default for PeerRpcClientConfig {
    fn default() -> Self {
        Self {
            request_timeout_ms: 5_000,
            envelope_ttl_ms: 60_000,
            max_response_bytes: 1_048_576,
            retry_policy: PeerRpcRetryPolicy::default(),
        }
    }
}

/// HTTP request emitted through a [`PeerTransportProvider`].
#[derive(Clone, PartialEq, Eq)]
pub struct PeerRpcHttpRequest {
    /// HTTP method.
    pub method: String,
    /// Stable peer RPC endpoint path.
    pub path: String,
    /// Encoded request body.
    pub body: Vec<u8>,
    /// Optional bearer credential; debug output always redacts it.
    pub bearer_token: Option<String>,
    /// Per-attempt network timeout.
    pub timeout_ms: u64,
    /// Maximum accepted response body size.
    pub max_response_bytes: usize,
}

impl std::fmt::Debug for PeerRpcHttpRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PeerRpcHttpRequest")
            .field("method", &self.method)
            .field("path", &self.path)
            .field("body_bytes", &self.body.len())
            .field(
                "bearer_token",
                &self.bearer_token.as_ref().map(|_| "REDACTED"),
            )
            .field("timeout_ms", &self.timeout_ms)
            .field("max_response_bytes", &self.max_response_bytes)
            .finish()
    }
}

/// Bounded response returned by a [`PeerTransportProvider`].
#[derive(Clone, PartialEq, Eq)]
pub struct PeerRpcHttpResponse {
    /// HTTP status code.
    pub status_code: u16,
    /// Raw response body.
    pub body: Vec<u8>,
}

impl std::fmt::Debug for PeerRpcHttpResponse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PeerRpcHttpResponse")
            .field("status_code", &self.status_code)
            .field("body_bytes", &self.body.len())
            .finish()
    }
}

/// Peer transport provider used by RPC clients to reach a peer endpoint.
pub trait PeerTransportProvider: Send + Sync {
    /// Sends one request to the peer base URL.
    fn send(
        &self,
        base_url: &str,
        request: PeerRpcHttpRequest,
    ) -> Result<PeerRpcHttpResponse, PeerRpcError>;

    /// Sends one request with cooperative cancellation.
    fn send_cancellable(
        &self,
        base_url: &str,
        request: PeerRpcHttpRequest,
        cancellation: &CancellationToken,
    ) -> Result<PeerRpcHttpResponse, PeerRpcError> {
        if cancellation.is_cancelled() {
            return Err(PeerRpcError::EndpointUnavailable);
        }
        self.send(base_url, request)
    }
}

/// Authenticated client for stable peer query, command, and diagnostic endpoints.
pub struct PeerRpcClient<T, I> {
    pub(crate) source_identity: CoreIdentity,
    pub(crate) config: PeerRpcClientConfig,
    pub(crate) transport: T,
    pub(crate) token_issuer: I,
    pub(crate) cancellation: CancellationToken,
}
impl<T, I> PeerRpcClient<T, I>
where
    T: PeerTransportProvider,
    I: PeerRpcTokenIssuer,
{
    /// Creates an authenticated peer RPC client.
    pub fn new(
        source_identity: CoreIdentity,
        config: PeerRpcClientConfig,
        transport: T,
        token_issuer: I,
    ) -> Self {
        Self {
            source_identity,
            config,
            transport,
            token_issuer,
            cancellation: CancellationToken::new(),
        }
    }

    /// Replaces the shared cancellation token used by I/O and retry waits.
    pub fn with_cancellation_token(mut self, cancellation: CancellationToken) -> Self {
        self.cancellation = cancellation;
        self
    }

    /// Cancels active official transport I/O and future retries.
    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    /// Reports whether this client has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    /// Executes a peer query.
    pub fn query(
        &self,
        endpoint_url: &str,
        request: PeerRpcOutboundRequest,
    ) -> Result<PeerRpcResponse, PeerRpcError> {
        self.call_peer(endpoint_url, PeerRpcCallKind::Query, request)
    }

    /// Executes a peer command.
    pub fn command(
        &self,
        endpoint_url: &str,
        request: PeerRpcOutboundRequest,
    ) -> Result<PeerRpcResponse, PeerRpcError> {
        self.call_peer(endpoint_url, PeerRpcCallKind::Command, request)
    }

    /// Reads the authenticated peer health endpoint.
    pub fn health(&self, endpoint_url: &str) -> Result<PeerHealthResponse, PeerRpcError> {
        let request_id = format!("health-{}-{}", std::process::id(), now_ms());
        let token = self.token_issuer.issue_peer_token(
            &request_id,
            None,
            now_ms(),
            self.config.envelope_ttl_ms,
        )?;
        let response = self.send_with_retry(
            endpoint_url,
            PeerRpcHttpRequest {
                method: "GET".to_string(),
                path: PEER_HEALTH_PATH.to_string(),
                body: Vec::new(),
                bearer_token: Some(token),
                timeout_ms: self.config.request_timeout_ms,
                max_response_bytes: self.config.max_response_bytes,
            },
        )?;
        serde_json::from_slice(&response.body)
            .map_err(|error| PeerRpcError::InvalidResponse(error.to_string()))
    }

    /// Reads and returns the authenticated peer runtime manifest.
    /// Loads the versioned public advertisement exposed by a peer.
    pub fn advertisement(&self, endpoint_url: &str) -> Result<PeerAdvertisementV1, PeerRpcError> {
        let request_id = format!("manifest-{}-{}", std::process::id(), now_ms());
        let token = self.token_issuer.issue_peer_token(
            &request_id,
            None,
            now_ms(),
            self.config.envelope_ttl_ms,
        )?;
        let response = self.send_with_retry(
            endpoint_url,
            PeerRpcHttpRequest {
                method: "GET".to_string(),
                path: PEER_MANIFEST_PATH.to_string(),
                body: Vec::new(),
                bearer_token: Some(token),
                timeout_ms: self.config.request_timeout_ms,
                max_response_bytes: self.config.max_response_bytes,
            },
        )?;
        let response = serde_json::from_slice::<PeerManifestResponse>(&response.body)
            .map_err(|error| PeerRpcError::InvalidResponse(error.to_string()))?;
        Ok(response.advertisement)
    }

    fn build_envelope(&self, request: &PeerRpcOutboundRequest) -> PeerRpcEnvelope {
        let now = now_ms();
        let trace_id = request
            .trace
            .as_ref()
            .map(|trace| trace.trace_id.clone())
            .unwrap_or_else(|| request.request_id.clone());
        let mut envelope = PeerRpcEnvelope::new(
            request.request_id.clone(),
            trace_id,
            self.source_identity.core_id.clone(),
            request.target_core_id.clone(),
            self.source_identity.tenant_id.clone(),
            self.source_identity.cluster_id.clone(),
            now,
            now.saturating_add(self.config.envelope_ttl_ms.max(1)),
            next_outbound_nonce(&request.request_id, now),
            request.capability.clone(),
            request.payload.clone(),
            request.idempotency_key.clone(),
            request.trace.clone(),
        );
        envelope.protocol_version = self.source_identity.protocol_version;
        envelope
    }

    fn send_with_retry(
        &self,
        endpoint_url: &str,
        request: PeerRpcHttpRequest,
    ) -> Result<PeerRpcHttpResponse, PeerRpcError> {
        let attempts = self.config.retry_policy.max_attempts.max(1);
        let mut backoff_ms = self.config.retry_policy.initial_backoff_ms;
        let mut last_error = PeerRpcError::EndpointUnavailable;
        for attempt in 0..attempts {
            match self
                .transport
                .send_cancellable(endpoint_url, request.clone(), &self.cancellation)
            {
                Ok(response) if (200..300).contains(&response.status_code) => return Ok(response),
                Ok(response) => {
                    last_error = http_status_error(response.status_code, response.body);
                }
                Err(error) => last_error = error,
            }
            if attempt + 1 < attempts {
                if self
                    .cancellation
                    .wait_timeout(Duration::from_millis(backoff_ms))
                {
                    return Err(PeerRpcError::EndpointUnavailable);
                }
                backoff_ms = backoff_ms
                    .saturating_mul(2)
                    .min(self.config.retry_policy.max_backoff_ms);
            }
        }
        Err(last_error)
    }
}

impl<T, I> PeerRpcClientExecutor for PeerRpcClient<T, I>
where
    T: PeerTransportProvider,
    I: PeerRpcTokenIssuer,
{
    fn call_peer(
        &self,
        endpoint_url: &str,
        kind: PeerRpcCallKind,
        request: PeerRpcOutboundRequest,
    ) -> Result<PeerRpcResponse, PeerRpcError> {
        let configured_attempts = self.config.retry_policy.max_attempts.max(1);
        let attempts = if kind == PeerRpcCallKind::Command && request.idempotency_key.is_none() {
            1
        } else {
            configured_attempts
        };
        let mut backoff_ms = self.config.retry_policy.initial_backoff_ms;
        let mut last_error = PeerRpcError::EndpointUnavailable;

        for attempt in 0..attempts {
            if self.cancellation.is_cancelled() {
                return Err(PeerRpcError::EndpointUnavailable);
            }
            match self.call_peer_once(endpoint_url, kind, &request) {
                Ok(response) => return Ok(response),
                Err(error) => {
                    let retryable = peer_error_is_retryable(&error);
                    last_error = error;
                    if !retryable {
                        break;
                    }
                }
            }
            if attempt + 1 < attempts {
                if self
                    .cancellation
                    .wait_timeout(Duration::from_millis(backoff_ms))
                {
                    return Err(PeerRpcError::EndpointUnavailable);
                }
                backoff_ms = backoff_ms
                    .saturating_mul(2)
                    .min(self.config.retry_policy.max_backoff_ms);
            }
        }
        Err(last_error)
    }
}

impl<T, I> PeerRpcClient<T, I>
where
    T: PeerTransportProvider,
    I: PeerRpcTokenIssuer,
{
    fn call_peer_once(
        &self,
        endpoint_url: &str,
        kind: PeerRpcCallKind,
        request: &PeerRpcOutboundRequest,
    ) -> Result<PeerRpcResponse, PeerRpcError> {
        let envelope = self.build_envelope(request);
        let envelope_hash = envelope_signing_hash(&envelope);
        let token = self.token_issuer.issue_peer_token(
            &envelope.request_id,
            Some(&envelope_hash),
            now_ms(),
            self.config.envelope_ttl_ms,
        )?;
        let body = serde_json::to_vec(&envelope)
            .map_err(|error| PeerRpcError::InvalidEnvelope(error.to_string()))?;
        let response = self.transport.send_cancellable(
            endpoint_url,
            PeerRpcHttpRequest {
                method: "POST".to_string(),
                path: match kind {
                    PeerRpcCallKind::Query => PEER_QUERY_PATH,
                    PeerRpcCallKind::Command => PEER_COMMAND_PATH,
                }
                .to_string(),
                body,
                bearer_token: Some(token),
                timeout_ms: self.config.request_timeout_ms,
                max_response_bytes: self.config.max_response_bytes,
            },
            &self.cancellation,
        )?;
        if !(200..300).contains(&response.status_code) {
            return Err(http_status_error(response.status_code, response.body));
        }
        let response = serde_json::from_slice::<PeerRpcResponse>(&response.body)
            .map_err(|error| PeerRpcError::InvalidResponse(error.to_string()))?;
        if response.request_id != request.request_id {
            return Err(PeerRpcError::InvalidResponse(
                "peer response request_id mismatch".to_string(),
            ));
        }
        Ok(response)
    }
}

fn next_outbound_nonce(request_id: &str, now_ms: u64) -> String {
    // appcore-norm: allow(global-state) reason: atomic sequence prevents process-local nonce reuse
    static NONCE_COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = NONCE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "{}-{}-{}-{}",
        request_id,
        now_ms,
        std::process::id(),
        counter
    )
}

fn peer_error_is_retryable(error: &PeerRpcError) -> bool {
    matches!(
        error,
        PeerRpcError::EndpointUnavailable | PeerRpcError::Transport(_)
    )
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
