// =============================================================================
//        #######
//     ###       ###     F: router.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::execution_queue::ExecutionQueue;
#[cfg(feature = "swarm")]
use crate::execution_route::ExecutionRoute;
use crate::execution_route::PlannedRoute;
use crate::model_load::ModelLoadCoordinator;
use crate::router_execution::{ObservedStreamSink, ResponseMode};
use crate::router_local::LocalRoutePlan;
use crate::router_support::{check_cancel_deadline, finalize_response, model_candidates};
use crate::{
    AiError, AiExecutionMode, AiLimits, AiObservationSink, AiRequest, AiResponse, AiResult,
    AiRuntimeHealth, AiTelemetry, AiTelemetrySnapshot, BackendRegistry, CancellationToken,
    CostScheduler, ExecutionAttempt, ExecutionQueueConfig, ExecutionQueueSnapshot,
    LightweightOutcome, LightweightResolver, ModelAdmission, ModelRecord, ModelRegistry,
    PlacementContext, PlacementPlanner, RouteReason,
};
#[cfg(feature = "swarm")]
use crate::{ComputeTarget, PlacementCandidate, PlacementKey, SwarmBridge};
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};
use std::sync::Arc;
use std::time::Instant;

/// Main backend-neutral resolution facade.
pub struct AiRuntime {
    limits: AiLimits,
    lightweight: Arc<dyn LightweightResolver>,
    pub(crate) models: Arc<ModelRegistry>,
    pub(crate) backends: Arc<BackendRegistry>,
    pub(crate) admission: Arc<dyn ModelAdmission>,
    planner: Arc<dyn PlacementPlanner>,
    execution_queue: Arc<ExecutionQueue>,
    pub(crate) model_loads: ModelLoadCoordinator,
    pub(crate) telemetry: Arc<AiTelemetry>,
    swarm_available: bool,
    #[cfg(feature = "swarm")]
    pub(crate) swarm: Option<Arc<dyn SwarmBridge>>,
}

impl Debug for AiRuntime {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AiRuntime")
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

impl AiRuntime {
    /// Composes the lightweight path, registries and mandatory admission policy.
    pub fn new(
        limits: AiLimits,
        lightweight: Arc<dyn LightweightResolver>,
        models: Arc<ModelRegistry>,
        backends: Arc<BackendRegistry>,
        admission: Arc<dyn ModelAdmission>,
    ) -> AiResult<Self> {
        if limits.max_attempts == 0 {
            return Err(AiError::InvalidInput("resolve attempt limit"));
        }
        Ok(Self {
            limits,
            lightweight,
            models,
            backends,
            admission,
            planner: Arc::new(CostScheduler::default()),
            execution_queue: Arc::new(ExecutionQueue::new(ExecutionQueueConfig::default())?),
            model_loads: ModelLoadCoordinator::default(),
            telemetry: Arc::new(AiTelemetry::default()),
            swarm_available: false,
            #[cfg(feature = "swarm")]
            swarm: None,
        })
    }

    /// Replaces the deterministic default cost planner.
    #[must_use]
    pub fn with_planner(mut self, planner: Arc<dyn PlacementPlanner>) -> Self {
        self.planner = planner;
        self
    }

    /// Replaces backend-route admission with explicit concurrency and waiting bounds.
    pub fn with_execution_queue(mut self, config: ExecutionQueueConfig) -> AiResult<Self> {
        self.execution_queue = Arc::new(ExecutionQueue::new(config)?);
        Ok(self)
    }

    /// Returns current bounded backend-route admission state.
    #[must_use]
    pub fn execution_queue(&self) -> ExecutionQueueSnapshot {
        self.execution_queue.snapshot()
    }

    /// Returns bounded health used by an AppCore composition adapter.
    pub fn health(&self) -> AiResult<AiRuntimeHealth> {
        Ok(AiRuntimeHealth {
            backends: self.backends.snapshot()?,
            models: self.models.snapshot()?,
            execution: self.execution_queue.snapshot(),
        })
    }

    /// Connects payload-free observations to an `appcore-ops` composition adapter.
    #[must_use]
    pub fn with_observation_sink(mut self, sink: Arc<dyn AiObservationSink>) -> Self {
        self.telemetry = Arc::new(AiTelemetry::new(sink));
        self
    }

    /// Returns current low-cardinality metrics without exposing request content.
    #[must_use]
    pub fn telemetry(&self) -> AiTelemetrySnapshot {
        self.telemetry.snapshot()
    }

    /// Installs an adapter backed by existing AppCore security, discovery and Peer RPC.
    #[cfg(feature = "swarm")]
    #[must_use]
    pub fn with_swarm_bridge(mut self, bridge: Arc<dyn SwarmBridge>) -> Self {
        self.swarm = Some(bridge);
        self.swarm_available = true;
        self
    }

    /// Resolves a request with a fresh cooperative cancellation token.
    pub async fn resolve(&self, request: AiRequest) -> AiResult<AiResponse> {
        self.resolve_with_cancellation(request, CancellationToken::new())
            .await
    }

    /// Resolves a request with caller-owned cooperative cancellation.
    pub async fn resolve_with_cancellation(
        &self,
        request: AiRequest,
        cancellation: CancellationToken,
    ) -> AiResult<AiResponse> {
        self.resolve_mode(request, cancellation, ResponseMode::Complete)
            .await
    }

    /// Resolves a request while applying synchronous backpressure per output event.
    pub async fn resolve_stream(
        &self,
        request: AiRequest,
        sink: &dyn crate::AiStreamSink,
    ) -> AiResult<AiResponse> {
        self.resolve_stream_with_cancellation(request, CancellationToken::new(), sink)
            .await
    }

    /// Resolves a streaming request with caller-owned cooperative cancellation.
    pub async fn resolve_stream_with_cancellation(
        &self,
        request: AiRequest,
        cancellation: CancellationToken,
        sink: &dyn crate::AiStreamSink,
    ) -> AiResult<AiResponse> {
        let sink = ObservedStreamSink::new(sink);
        self.resolve_mode(request, cancellation, ResponseMode::Stream(&sink))
            .await
    }

    async fn resolve_mode(
        &self,
        request: AiRequest,
        cancellation: CancellationToken,
        mode: ResponseMode<'_>,
    ) -> AiResult<AiResponse> {
        let observed_at = Instant::now();
        self.telemetry
            .request_started(&request.task, request.options.execution);
        let mut attempts = 0;
        let result = self
            .resolve_inner(request, cancellation, &mut attempts, mode)
            .await;
        self.telemetry
            .completed(result.is_ok(), observed_at.elapsed(), attempts);
        result
    }

    async fn resolve_inner(
        &self,
        request: AiRequest,
        cancellation: CancellationToken,
        observed_attempts: &mut usize,
        mode: ResponseMode<'_>,
    ) -> AiResult<AiResponse> {
        request.validate(self.limits)?;
        if cancellation.is_cancelled() {
            return Err(AiError::Cancelled);
        }
        if request.options.execution == AiExecutionMode::Swarm && !self.swarm_available {
            return Err(AiError::SwarmUnavailable);
        }
        let started = Instant::now();
        let mut fallback = None;
        if request.options.execution != AiExecutionMode::Swarm
            && self.lightweight.can_handle(&request)
        {
            match self.lightweight.resolve(&request, &cancellation)? {
                LightweightOutcome::Handled {
                    response, escalate, ..
                } if escalate && request.options.allow_escalation => {
                    self.telemetry.fallback();
                    *observed_attempts = 1;
                    fallback = Some(response);
                }
                LightweightOutcome::Handled { response, .. } => {
                    self.telemetry.route_selected(
                        &crate::ExecutionTarget::Lightweight,
                        None,
                        false,
                    );
                    *observed_attempts = 1;
                    mode.emit_complete(&response, &cancellation)?;
                    return Ok(response);
                }
                LightweightOutcome::NotHandled { .. } => {}
            }
        }
        let candidates = model_candidates(&self.models, &request)?;
        let queue_deadline = request
            .options
            .deadline
            .map(|deadline| deadline.saturating_sub(started.elapsed()));
        let _permit = self.execution_queue.acquire(
            request.options.priority,
            queue_deadline,
            cancellation.clone(),
        )?;
        match self
            .execute_candidates(
                &request,
                &cancellation,
                started,
                candidates,
                observed_attempts,
                mode,
            )
            .await
        {
            Ok(response) => Ok(response),
            Err(AiError::NotFound(_)) | Err(AiError::Capacity(_)) if fallback.is_some() => {
                let response = fallback.ok_or(AiError::NotFound("AI route"))?;
                mode.emit_complete(&response, &cancellation)?;
                Ok(response)
            }
            Err(error) => Err(error),
        }
    }

    async fn execute_candidates(
        &self,
        request: &AiRequest,
        cancellation: &CancellationToken,
        started: Instant,
        candidates: Vec<ModelRecord>,
        observed_attempts: &mut usize,
        mode: ResponseMode<'_>,
    ) -> AiResult<AiResponse> {
        let mut attempts = Vec::new();
        let (routes, capacity_limited) = self.plan_routes(request, started, candidates)?;
        for planned in routes {
            if attempts.len() >= self.limits.max_attempts {
                return Err(AiError::LimitExceeded {
                    kind: crate::LimitKind::Attempts,
                    actual: u64::try_from(attempts.len().saturating_add(1)).unwrap_or(u64::MAX),
                    limit: u64::try_from(self.limits.max_attempts).unwrap_or(u64::MAX),
                });
            }
            check_cancel_deadline(request, cancellation, started)?;
            let target = planned.route.target();
            let reason = if attempts.is_empty() {
                RouteReason::LowestAdmittedCost
            } else {
                RouteReason::Escalated
            };
            attempts.push(ExecutionAttempt {
                sequence: attempts.len() + 1,
                target: target.clone(),
                reason,
                estimated_cost_units: planned.score,
            });
            *observed_attempts = attempts.len();
            self.telemetry
                .route_selected(&target, planned.route.device_kind(), attempts.len() > 1);
            match self
                .execute_route(request, cancellation, &planned.route, mode)
                .await
            {
                Ok(response) => {
                    check_cancel_deadline(request, cancellation, started)?;
                    return finalize_response(response, request, target, attempts, self.limits);
                }
                Err(error)
                    if request.options.allow_escalation
                        && error.is_transient()
                        && mode.can_escalate_after_error() => {}
                Err(error) => return Err(error),
            }
        }
        if request.options.execution == AiExecutionMode::Swarm && !capacity_limited {
            Err(AiError::SwarmUnavailable)
        } else if capacity_limited {
            Err(AiError::Capacity("all model routes were denied"))
        } else {
            Err(AiError::NotFound("compatible AI route"))
        }
    }

    fn plan_routes(
        &self,
        request: &AiRequest,
        started: Instant,
        models: Vec<ModelRecord>,
    ) -> AiResult<(Vec<PlannedRoute>, bool)> {
        let allow_peer = request.options.distribution.allow_remote_storage;
        let LocalRoutePlan {
            routes,
            candidates,
            mut capacity_limited,
            pressure_limited,
        } = if request.options.execution == AiExecutionMode::Swarm {
            LocalRoutePlan::default()
        } else {
            self.local_routes(request, &models, allow_peer)?
        };
        #[cfg(feature = "swarm")]
        let (mut routes, mut candidates) = (routes, candidates);
        #[cfg(feature = "swarm")]
        self.add_swarm_routes(request, started, &models, &mut routes, &mut candidates)?;
        let deadline_remaining = request
            .options
            .deadline
            .map(|deadline| deadline.saturating_sub(started.elapsed()));
        let plan = self.planner.plan(
            PlacementContext {
                priority: request.options.priority,
                latency_class: request.options.latency,
                resource_mode: request.options.resources,
                deadline_remaining,
                allow_remote: request.options.execution != AiExecutionMode::Local
                    && request.options.distribution.allow_remote_compute,
                prefer_local: request.options.distribution.prefer_local,
                max_remote_latency: request.options.distribution.max_remote_latency,
                pressure_limited,
            },
            &candidates,
        );
        let mut routes = routes
            .into_iter()
            .map(|route| (route.key().clone(), route))
            .collect::<BTreeMap<_, _>>();
        let mut ordered = Vec::with_capacity(plan.ordered.len());
        for scored in plan.ordered {
            if request
                .options
                .max_cost_units
                .is_some_and(|maximum| scored.score > maximum)
            {
                capacity_limited = true;
                continue;
            }
            if let Some(route) = routes.remove(&scored.key) {
                ordered.push(PlannedRoute {
                    route,
                    score: scored.score,
                });
            }
        }
        Ok((ordered, capacity_limited || !plan.rejected.is_empty()))
    }

    #[cfg(feature = "swarm")]
    fn add_swarm_routes(
        &self,
        request: &AiRequest,
        started: Instant,
        models: &[ModelRecord],
        routes: &mut Vec<ExecutionRoute>,
        candidates: &mut Vec<PlacementCandidate>,
    ) -> AiResult<()> {
        if request.options.execution == AiExecutionMode::Local
            || !request.options.distribution.allow_remote_compute
        {
            return Ok(());
        }
        let bridge = self.swarm.as_ref().ok_or(AiError::SwarmUnavailable)?;
        let tenant = &request
            .options
            .authorization
            .as_ref()
            .ok_or(AiError::Unauthorized)?
            .tenant;
        for model in models {
            let remote = bridge.routes(
                request,
                &model.descriptor,
                request.options.distribution.max_peers,
            )?;
            if remote.len() > request.options.distribution.max_peers {
                return Err(AiError::LimitExceeded {
                    kind: crate::LimitKind::Peers,
                    actual: u64::try_from(remote.len()).unwrap_or(u64::MAX),
                    limit: u64::try_from(request.options.distribution.max_peers)
                        .unwrap_or(u64::MAX),
                });
            }
            for route in remote {
                route.validate(&model.descriptor)?;
                if &route.tenant != tenant
                    || !crate::execution_route::remote_artifact_allowed(
                        &route,
                        request.options.distribution.allow_remote_storage,
                    )
                    || request
                        .options
                        .backend
                        .as_ref()
                        .is_some_and(|backend| backend != &route.backend)
                    || request
                        .options
                        .device
                        .as_ref()
                        .is_some_and(|device| device != &route.device)
                    || route.lease_remaining
                        <= started
                            .elapsed()
                            .saturating_add(std::time::Duration::from_millis(
                                route.rtt_ms.saturating_add(route.load_time_ms),
                            ))
                {
                    continue;
                }
                let key = PlacementKey {
                    model: model.descriptor.id.clone(),
                    backend: route.backend.clone(),
                    target: ComputeTarget::RemotePeer {
                        peer: route.peer.clone(),
                        device: route.device.clone(),
                        kind: route.kind,
                    },
                };
                candidates.push(PlacementCandidate {
                    key: key.clone(),
                    health: route.health,
                    resources: route.resources,
                    metrics: route.metrics,
                    model_resident: route.model_resident,
                    artifact_source: route.artifact_source.clone(),
                    load_time_ms: route.load_time_ms,
                    transfer_cost_units: route.transfer_cost_units,
                    inference_cost_units: route.inference_cost_units,
                    rtt_ms: Some(route.rtt_ms),
                    bandwidth_bytes_per_second: route.bandwidth_bytes_per_second,
                    trusted: true,
                    failover_cost_units: route.failover_cost_units,
                });
                routes.push(ExecutionRoute::Remote {
                    key,
                    route: Box::new(route),
                });
            }
        }
        Ok(())
    }
}
