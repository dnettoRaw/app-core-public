// =============================================================================
//        #######
//     ###       ###     F: client.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::*;
use crate::worker::ControlPlaneWorker;

/// Retry limits and exponential backoff bounds for control-plane requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Maximum number of attempts, including the initial request.
    pub max_attempts: usize,
    /// Delay before the first retry.
    pub initial_backoff_ms: u64,
    /// Maximum delay between attempts.
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlPlaneHttpConfig {
    /// Base URL that hosts the stable control-plane endpoints.
    pub base_url: String,
    /// Per-attempt network timeout.
    pub timeout_ms: u64,
    /// Retry behavior for transport and non-success responses.
    pub retry_policy: RetryPolicy,
}

/// Transport request produced by [`HttpControlPlaneClient`].
#[derive(Debug, Clone, PartialEq, Eq)]
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
}

/// Control-plane client that maps stable contracts onto an HTTP transport.
#[derive(Debug)]
pub struct HttpControlPlaneClient<T> {
    config: ControlPlaneHttpConfig,
    transport: Arc<T>,
    worker: ControlPlaneWorker,
    cancellation: CancellationToken,
}

impl<T> Clone for HttpControlPlaneClient<T> {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            transport: Arc::clone(&self.transport),
            worker: self.worker.clone(),
            cancellation: self.cancellation.clone(),
        }
    }
}

impl<T> HttpControlPlaneClient<T>
where
    T: HttpTransport,
{
    /// Creates an HTTP client with a dedicated bounded worker queue.
    pub fn new(config: ControlPlaneHttpConfig, transport: T) -> Self {
        Self {
            config,
            transport: Arc::new(transport),
            worker: ControlPlaneWorker::new(),
            cancellation: CancellationToken::new(),
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
        let body = serde_json::to_vec(value)
            .map_err(|error| ControlPlaneError::Transport(error.to_string()))?;
        let request = HttpControlPlaneRequest {
            method: "POST".to_string(),
            path: path.to_string(),
            body,
            timeout_ms: self.config.timeout_ms,
        };
        self.send_with_retry::<Resp>(request, trace)
    }

    fn send_with_retry<Resp>(
        &self,
        request: HttpControlPlaneRequest,
        trace: Option<&TraceContext>,
    ) -> ControlPlaneResult<Resp>
    where
        Resp: for<'de> Deserialize<'de>,
    {
        let attempts = self.config.retry_policy.max_attempts.max(1);
        let mut backoff_ms = self.config.retry_policy.initial_backoff_ms;
        let mut last_error = ControlPlaneError::Offline;
        for attempt in 0..attempts {
            match self.transport.send_json_traced_cancellable(
                &self.config.base_url,
                request.clone(),
                trace,
                &self.cancellation,
            ) {
                Ok(response) if (200..300).contains(&response.status_code) => {
                    return serde_json::from_slice(&response.body)
                        .map_err(|error| ControlPlaneError::InvalidResponse(error.to_string()));
                }
                Ok(response) => {
                    last_error = ControlPlaneError::Rejected(format!(
                        "http_status={}",
                        response.status_code
                    ));
                }
                Err(error) => last_error = error,
            }

            if attempt + 1 < attempts {
                if self
                    .cancellation
                    .wait_timeout(Duration::from_millis(backoff_ms))
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

impl<T> ControlPlaneProvider for HttpControlPlaneClient<T>
where
    T: HttpTransport + 'static,
{
    fn register<'a>(
        &'a self,
        registration: CoreRegistration,
    ) -> ControlPlaneFuture<'a, CorePresence> {
        let client = self.clone();
        self.worker
            .enqueue(move || client.post(CONTROL_REGISTER_PATH, &registration))
    }

    fn heartbeat<'a>(
        &'a self,
        request: HeartbeatRequest,
    ) -> ControlPlaneFuture<'a, HeartbeatResponse> {
        let client = self.clone();
        self.worker
            .enqueue(move || client.post(CONTROL_HEARTBEAT_PATH, &request))
    }

    fn discover_peers<'a>(
        &'a self,
        identity: &'a CoreIdentity,
    ) -> ControlPlaneFuture<'a, PeerDirectory> {
        let client = self.clone();
        let identity = identity.clone();
        self.worker
            .enqueue(move || client.post(CONTROL_PEERS_PATH, &identity))
    }

    fn acquire_or_renew_service_lease<'a>(
        &'a self,
        identity: &'a CoreIdentity,
        service_id: &'a ServiceId,
        ttl_ms: u64,
        now_ms: u64,
    ) -> ControlPlaneFuture<'a, ServiceLeaderLease> {
        let client = self.clone();
        let request = ServiceLeaseRequest {
            identity: identity.clone(),
            service_id: service_id.clone(),
            ttl_ms,
            now_ms,
        };
        self.worker
            .enqueue(move || client.post(CONTROL_SERVICE_LEASE_PATH, &request))
    }

    fn release_service_lease<'a>(
        &'a self,
        lease: ServiceLeaderLease,
    ) -> ControlPlaneFuture<'a, ()> {
        let client = self.clone();
        self.worker.enqueue(move || {
            let _: EmptyResponse = client.post(CONTROL_SERVICE_LEASE_RELEASE_PATH, &lease)?;
            Ok(())
        })
    }

    fn register_traced<'a>(
        &'a self,
        registration: CoreRegistration,
        trace: Option<&'a TraceContext>,
    ) -> ControlPlaneFuture<'a, CorePresence> {
        let client = self.clone();
        let trace = trace.cloned();
        self.worker.enqueue(move || {
            client.post_traced(CONTROL_REGISTER_PATH, &registration, trace.as_ref())
        })
    }

    fn heartbeat_traced<'a>(
        &'a self,
        request: HeartbeatRequest,
        trace: Option<&'a TraceContext>,
    ) -> ControlPlaneFuture<'a, HeartbeatResponse> {
        let client = self.clone();
        let trace = trace.cloned();
        self.worker
            .enqueue(move || client.post_traced(CONTROL_HEARTBEAT_PATH, &request, trace.as_ref()))
    }

    fn discover_peers_traced<'a>(
        &'a self,
        identity: &'a CoreIdentity,
        trace: Option<&'a TraceContext>,
    ) -> ControlPlaneFuture<'a, PeerDirectory> {
        let client = self.clone();
        let identity = identity.clone();
        let trace = trace.cloned();
        self.worker
            .enqueue(move || client.post_traced(CONTROL_PEERS_PATH, &identity, trace.as_ref()))
    }

    fn acquire_or_renew_service_lease_traced<'a>(
        &'a self,
        identity: &'a CoreIdentity,
        service_id: &'a ServiceId,
        ttl_ms: u64,
        now_ms: u64,
        trace: Option<&'a TraceContext>,
    ) -> ControlPlaneFuture<'a, ServiceLeaderLease> {
        let client = self.clone();
        let trace = trace.cloned();
        let request = ServiceLeaseRequest {
            identity: identity.clone(),
            service_id: service_id.clone(),
            ttl_ms,
            now_ms,
        };
        self.worker.enqueue(move || {
            client.post_traced(CONTROL_SERVICE_LEASE_PATH, &request, trace.as_ref())
        })
    }

    fn release_service_lease_traced<'a>(
        &'a self,
        lease: ServiceLeaderLease,
        trace: Option<&'a TraceContext>,
    ) -> ControlPlaneFuture<'a, ()> {
        let client = self.clone();
        let trace = trace.cloned();
        self.worker.enqueue(move || {
            let _: EmptyResponse =
                client.post_traced(CONTROL_SERVICE_LEASE_RELEASE_PATH, &lease, trace.as_ref())?;
            Ok(())
        })
    }
}
