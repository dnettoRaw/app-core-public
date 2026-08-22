// =============================================================================
//        #######
//     ###       ###     F: openai_backend.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::openai_codec;
use crate::{
    AiContent, AiError, AiRequest, AiResponse, AiResult, BackendDescriptor, BackendFuture,
    BackendHealth, CancellationToken, DeviceId, DeviceKind, InferenceBackend, ModelDescriptor,
    OpenAiCompatibleConfig, OpenAiCompatibleTransport, OpenAiTransportRequest, ResourceEstimate,
    ResourceEstimateBreakdown,
};
use std::fmt::{Debug, Formatter};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

/// Real generative adapter for bounded OpenAI-compatible HTTP servers.
pub struct OpenAiCompatibleBackend {
    config: OpenAiCompatibleConfig,
    descriptor: BackendDescriptor,
    transport: Arc<dyn OpenAiCompatibleTransport>,
    health: AtomicU8,
}

impl OpenAiCompatibleBackend {
    /// Creates an adapter using an explicit transport composition boundary.
    pub fn new(
        config: OpenAiCompatibleConfig,
        transport: Arc<dyn OpenAiCompatibleTransport>,
    ) -> AiResult<Self> {
        config.validate()?;
        let descriptor = BackendDescriptor {
            id: config.backend_id.clone(),
            tasks: config.tasks(),
            input_modalities: config.modalities(),
            formats: config.formats(),
            devices: config.devices.clone(),
            costs: config.costs(),
        };
        descriptor.validate()?;
        Ok(Self {
            config,
            descriptor,
            transport,
            health: AtomicU8::new(0),
        })
    }

    /// Updates health from a composition-owned probe or supervisor adapter.
    pub fn set_health(&self, health: BackendHealth) {
        self.health.store(encode_health(health), Ordering::Release);
    }

    fn server_model<'a>(&'a self, model: &ModelDescriptor) -> AiResult<&'a str> {
        self.config
            .model_names
            .get(&model.id)
            .map(String::as_str)
            .ok_or(AiError::NotFound("OpenAI-compatible model binding"))
    }

    fn check_capabilities(&self, request: &AiRequest) -> AiResult<()> {
        let generation = &request.options.generation;
        if !generation.tools.is_empty() && !self.config.capabilities.tools {
            return Err(AiError::Unsupported("OpenAI-compatible tools"));
        }
        if generation.seed.is_some() && !self.config.capabilities.seed {
            return Err(AiError::Unsupported("OpenAI-compatible seed"));
        }
        if !generation.stop_sequences.is_empty() && !self.config.capabilities.stop_sequences {
            return Err(AiError::Unsupported("OpenAI-compatible stop sequences"));
        }
        if request.input.parts().iter().any(|part| {
            matches!(part, AiContent::Binary { media_type, .. } if media_type.starts_with("image/"))
        }) && !self.config.capabilities.vision
        {
            return Err(AiError::Unsupported("OpenAI-compatible vision"));
        }
        Ok(())
    }
}

impl Debug for OpenAiCompatibleBackend {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenAiCompatibleBackend")
            .field("backend", &self.descriptor.id)
            .field("engine", &self.config.engine)
            .field("endpoint", &"REDACTED")
            .field("models", &self.config.model_names.len())
            .field("health", &self.health())
            .finish_non_exhaustive()
    }
}

impl InferenceBackend for OpenAiCompatibleBackend {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn health(&self) -> BackendHealth {
        decode_health(self.health.load(Ordering::Acquire))
    }

    fn estimate(
        &self,
        request: &AiRequest,
        model: &ModelDescriptor,
        device: &DeviceId,
    ) -> AiResult<ResourceEstimate> {
        let kind = self
            .descriptor
            .devices
            .iter()
            .find(|candidate| &candidate.id == device)
            .map(|candidate| candidate.kind)
            .ok_or(AiError::NotFound("OpenAI-compatible device"))?;
        let input_bytes = request.input.parts().iter().fold(0u64, |total, part| {
            let bytes = match part {
                AiContent::Text(value) => value.len(),
                AiContent::Message(message) => message.content.len(),
                AiContent::Binary { media_type, bytes } => {
                    media_type.len().saturating_add(bytes.len())
                }
            };
            total.saturating_add(u64::try_from(bytes).unwrap_or(u64::MAX))
        });
        let batch_memory = u64::try_from(request.options.generation.max_output_tokens)
            .unwrap_or(u64::MAX)
            .saturating_mul(16);
        Ok(ResourceEstimateBreakdown {
            cpu_percent: if kind == DeviceKind::Cpu { 100 } else { 25 },
            gpu_percent: if kind == DeviceKind::Gpu { 100 } else { 0 },
            model_memory_bytes: model.estimated_memory_bytes,
            runtime_memory_bytes: input_bytes.saturating_mul(3),
            batch_memory_bytes: batch_memory,
            model_vram_bytes: model.estimated_vram_bytes,
            workers: 1,
            ..ResourceEstimateBreakdown::default()
        }
        .peak())
    }

    fn load<'a>(
        &'a self,
        model: &'a ModelDescriptor,
        cancellation: &'a CancellationToken,
    ) -> BackendFuture<'a, ()> {
        Box::pin(async move {
            if cancellation.is_cancelled() {
                return Err(AiError::Cancelled);
            }
            let _ = self.server_model(model)?;
            if self.health() == BackendHealth::Unavailable {
                return Err(AiError::BackendUnavailable(self.descriptor.id.clone()));
            }
            Ok(())
        })
    }

    fn unload<'a>(
        &'a self,
        model: &'a ModelDescriptor,
        cancellation: &'a CancellationToken,
    ) -> BackendFuture<'a, ()> {
        Box::pin(async move {
            if cancellation.is_cancelled() {
                return Err(AiError::Cancelled);
            }
            let _ = self.server_model(model)?;
            Ok(())
        })
    }

    fn infer<'a>(
        &'a self,
        request: &'a AiRequest,
        model: &'a ModelDescriptor,
        _device: &'a DeviceId,
        cancellation: &'a CancellationToken,
    ) -> BackendFuture<'a, AiResponse> {
        Box::pin(async move {
            if self.health() == BackendHealth::Unavailable {
                return Err(AiError::BackendUnavailable(self.descriptor.id.clone()));
            }
            self.check_capabilities(request)?;
            let body = openai_codec::encode(request, self.server_model(model)?)?;
            if body.len() > self.config.max_request_bytes {
                return Err(AiError::LimitExceeded {
                    kind: crate::LimitKind::InputBytes,
                    actual: u64::try_from(body.len()).unwrap_or(u64::MAX),
                    limit: u64::try_from(self.config.max_request_bytes).unwrap_or(u64::MAX),
                });
            }
            let transport_request = OpenAiTransportRequest::new(
                self.descriptor.id.clone(),
                self.config.base_url.clone(),
                self.config.request_path.clone(),
                body,
                request
                    .options
                    .deadline
                    .map_or(self.config.timeout, |deadline| {
                        deadline.min(self.config.timeout)
                    }),
                self.config.max_response_bytes,
                request.options.credential.clone(),
            );
            let response = self.transport.send(&transport_request, cancellation)?;
            if !(200..300).contains(&response.status_code) {
                return Err(AiError::BackendFailure {
                    backend: self.descriptor.id.clone(),
                    code: "http-status",
                });
            }
            openai_codec::decode(&response.body, self.config.max_response_bytes)
        })
    }
}

fn encode_health(health: BackendHealth) -> u8 {
    match health {
        BackendHealth::Healthy => 0,
        BackendHealth::Degraded => 1,
        BackendHealth::Unavailable => 2,
    }
}

fn decode_health(value: u8) -> BackendHealth {
    match value {
        0 => BackendHealth::Healthy,
        1 => BackendHealth::Degraded,
        _ => BackendHealth::Unavailable,
    }
}
