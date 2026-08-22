// =============================================================================
//        #######
//     ###       ###     F: router_support.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{
    AiError, AiLimits, AiRequest, AiResponse, AiResult, CancellationToken, ExecutionAttempt,
    ExecutionDecision, ExecutionTarget, ModelDescriptor, ModelRecord, ModelRegistry, ModelState,
    QualityTier, ResourceEstimate, RouteReason,
};
use std::time::Instant;

pub(crate) fn request_input_bytes(request: &AiRequest) -> usize {
    request.input.parts().iter().fold(0usize, |total, part| {
        let bytes = match part {
            crate::AiContent::Text(value) => value.len(),
            crate::AiContent::Message(message) => message.content.len(),
            crate::AiContent::Binary { media_type, bytes } => {
                media_type.len().saturating_add(bytes.len())
            }
        };
        total.saturating_add(bytes)
    })
}

pub(crate) fn model_candidates(
    models: &ModelRegistry,
    request: &AiRequest,
) -> AiResult<Vec<ModelRecord>> {
    let input_bytes = request_input_bytes(request);
    let modalities = request.input.modalities();
    let mut candidates = if let Some(required) = &request.options.model {
        vec![models.get(required)?]
    } else {
        models.candidates(&request.task)?
    };
    candidates.retain(|record| {
        matches!(
            record.state,
            ModelState::Available | ModelState::Loading | ModelState::Ready
        ) && record.descriptor.max_input_bytes >= input_bytes
            && record.descriptor.supports_modalities(&modalities)
            && (request.options.model.is_some()
                || record
                    .descriptor
                    .quality
                    .is_some_and(|quality| quality >= request.options.quality.minimum_tier()))
    });
    candidates.sort_by_key(|record| {
        (
            record.descriptor.quality.unwrap_or(QualityTier::Large),
            record.descriptor.load_cost_units,
            record.descriptor.id.clone(),
        )
    });
    Ok(candidates)
}

pub(crate) fn bounded_estimate(
    mut estimate: ResourceEstimate,
    model: &ModelDescriptor,
) -> ResourceEstimate {
    estimate.memory_bytes = estimate.memory_bytes.max(model.estimated_memory_bytes);
    estimate.vram_bytes = estimate.vram_bytes.max(model.estimated_vram_bytes);
    estimate.workers = estimate.workers.max(1);
    estimate
}

pub(crate) fn check_cancel_deadline(
    request: &AiRequest,
    cancellation: &CancellationToken,
    started: Instant,
) -> AiResult<()> {
    if cancellation.is_cancelled() {
        return Err(AiError::Cancelled);
    }
    if request
        .options
        .deadline
        .is_some_and(|deadline| started.elapsed() >= deadline)
    {
        return Err(AiError::DeadlineExceeded);
    }
    Ok(())
}

pub(crate) fn finalize_response(
    response: AiResponse,
    request: &AiRequest,
    selected: ExecutionTarget,
    attempts: Vec<ExecutionAttempt>,
    limits: AiLimits,
) -> AiResult<AiResponse> {
    let decision = request
        .options
        .include_diagnostics
        .then(|| ExecutionDecision {
            selected,
            reason: if attempts.len() > 1 {
                RouteReason::Escalated
            } else if request.options.backend.is_some() || request.options.model.is_some() {
                RouteReason::ForcedOverride
            } else {
                RouteReason::LowestAdmittedCost
            },
            attempts,
        });
    AiResponse::new(response.output, response.metadata, decision, limits)
}
