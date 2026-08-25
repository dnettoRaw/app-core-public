// =============================================================================
//        #######
//     ###       ###     F: router_execution.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/25 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/25 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.2
// =============================================================================

use crate::execution_route::ExecutionRoute;
use crate::model_load::ModelLoadAdmission;
use crate::{
    AiError, AiRequest, AiResponse, AiResult, AiRuntime, AiStreamSink, CancellationToken,
    ModelRecord,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

pub(crate) struct ObservedStreamSink<'a> {
    sink: &'a dyn AiStreamSink,
    emitted: AtomicBool,
}

impl<'a> ObservedStreamSink<'a> {
    pub(crate) const fn new(sink: &'a dyn AiStreamSink) -> Self {
        Self {
            sink,
            emitted: AtomicBool::new(false),
        }
    }

    fn emitted(&self) -> bool {
        self.emitted.load(Ordering::Acquire)
    }
}

impl AiStreamSink for ObservedStreamSink<'_> {
    fn event(&self, event: &crate::AiStreamEvent) -> AiResult<()> {
        self.emitted.store(true, Ordering::Release);
        self.sink.event(event)
    }
}

#[derive(Clone, Copy)]
pub(crate) enum ResponseMode<'a> {
    Complete,
    Stream(&'a ObservedStreamSink<'a>),
}

impl ResponseMode<'_> {
    pub(crate) fn emit_complete(
        self,
        response: &AiResponse,
        cancellation: &CancellationToken,
    ) -> AiResult<()> {
        match self {
            Self::Complete => Ok(()),
            Self::Stream(sink) => crate::streaming::emit_complete(response, cancellation, sink),
        }
    }

    pub(crate) fn can_escalate_after_error(self) -> bool {
        match self {
            Self::Complete => true,
            Self::Stream(sink) => !sink.emitted(),
        }
    }
}

impl AiRuntime {
    pub(crate) async fn execute_route(
        &self,
        request: &AiRequest,
        cancellation: &CancellationToken,
        route: &ExecutionRoute,
        mode: ResponseMode<'_>,
    ) -> AiResult<AiResponse> {
        match route {
            ExecutionRoute::Local {
                model,
                backend,
                device,
                ..
            } => {
                self.execute_backend(request, cancellation, model, backend.as_ref(), device, mode)
                    .await
            }
            #[cfg(feature = "swarm")]
            ExecutionRoute::Remote { route, .. } => {
                let response = self
                    .swarm
                    .as_ref()
                    .ok_or(AiError::SwarmUnavailable)?
                    .execute(route, request, cancellation)
                    .await?;
                mode.emit_complete(&response, cancellation)?;
                Ok(response)
            }
        }
    }

    async fn execute_backend(
        &self,
        request: &AiRequest,
        cancellation: &CancellationToken,
        record: &ModelRecord,
        backend: &dyn crate::InferenceBackend,
        device: &crate::DeviceId,
        mode: ResponseMode<'_>,
    ) -> AiResult<AiResponse> {
        if let ModelLoadAdmission::Load(permit) = self.model_loads.acquire(
            &record.descriptor.id,
            &backend.descriptor().id,
            request.options.deadline,
            cancellation,
        )? {
            self.models.note_load_started(&record.descriptor.id)?;
            let load_started = Instant::now();
            let loaded = backend.load(&record.descriptor, cancellation).await;
            let success = loaded.is_ok();
            self.telemetry.model_load(load_started.elapsed(), success);
            self.models
                .note_load_finished(&record.descriptor.id, success)?;
            permit.complete(success)?;
            loaded?;
        }
        let result = match mode {
            ResponseMode::Complete => {
                backend
                    .infer(request, &record.descriptor, device, cancellation)
                    .await
            }
            ResponseMode::Stream(sink) => {
                backend
                    .infer_stream(request, &record.descriptor, device, cancellation, sink)
                    .await
            }
        };
        if matches!(&result, Err(AiError::BackendUnavailable(id)) if id == &backend.descriptor().id)
        {
            self.model_loads
                .invalidate(&record.descriptor.id, &backend.descriptor().id)?;
        }
        result
    }
}
