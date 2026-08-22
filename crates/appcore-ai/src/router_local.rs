// =============================================================================
//        #######
//     ###       ###     F: router_local.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::execution_route::{artifact_source, model_is_resident, ExecutionRoute};
use crate::router_support::bounded_estimate;
use crate::{
    AdmissionDecision, AiRequest, AiResult, ComputeTarget, ModelRecord, ModelState,
    PlacementCandidate, PlacementKey, PlacementMetrics,
};
use std::sync::Arc;

#[derive(Default)]
pub(crate) struct LocalRoutePlan {
    pub(crate) routes: Vec<ExecutionRoute>,
    pub(crate) candidates: Vec<PlacementCandidate>,
    pub(crate) capacity_limited: bool,
    pub(crate) pressure_limited: bool,
}

impl crate::AiRuntime {
    pub(crate) fn local_routes(
        &self,
        request: &AiRequest,
        models: &[ModelRecord],
        allow_peer: bool,
    ) -> AiResult<LocalRoutePlan> {
        let mut plan = LocalRoutePlan::default();
        let modalities = request.input.modalities();
        for record in models {
            let model = Arc::new(record.clone());
            for backend in
                self.backends
                    .candidates_with_modalities(request, &model.descriptor, &modalities)?
            {
                let descriptor = backend.descriptor();
                for device in descriptor.devices.iter().filter(|device| {
                    request
                        .options
                        .device
                        .as_ref()
                        .is_none_or(|required| required == &device.id)
                        && model.descriptor.supported_devices.contains(&device.kind)
                }) {
                    let estimate = bounded_estimate(
                        backend.estimate(request, &model.descriptor, &device.id)?,
                        &model.descriptor,
                    );
                    match self
                        .admission
                        .admit_on(request, estimate, device.kind, &device.id)?
                    {
                        AdmissionDecision::Admit { budget } => {
                            plan.pressure_limited |= budget.pressure_limited;
                        }
                        AdmissionDecision::Defer { .. } | AdmissionDecision::Reject { .. } => {
                            self.telemetry.admission_restricted();
                            plan.capacity_limited = true;
                            continue;
                        }
                    }
                    let key = PlacementKey {
                        model: model.descriptor.id.clone(),
                        backend: descriptor.id.clone(),
                        target: ComputeTarget::local(device.kind, device.id.clone()),
                    };
                    let resident = model_is_resident(&model, device.kind, &device.id);
                    let source = artifact_source(&model, device.kind, &device.id, allow_peer);
                    if model.state == ModelState::Available && source.is_none() {
                        continue;
                    }
                    let backend_metrics = backend.placement_metrics(&device.id)?;
                    let hardware_metrics =
                        self.admission.placement_metrics(device.kind, &device.id)?;
                    plan.candidates.push(PlacementCandidate {
                        key: key.clone(),
                        health: backend.health(),
                        resources: estimate,
                        metrics: hardware_metrics.map_or(backend_metrics, |hardware| {
                            merge_metrics(backend_metrics, hardware)
                        }),
                        model_resident: resident,
                        artifact_source: source,
                        load_time_ms: model
                            .descriptor
                            .load_cost_units
                            .saturating_add(descriptor.costs.load_units),
                        transfer_cost_units: if resident {
                            0
                        } else {
                            model.descriptor.load_cost_units
                        },
                        inference_cost_units: descriptor.costs.inference_units,
                        rtt_ms: None,
                        bandwidth_bytes_per_second: None,
                        trusted: true,
                        failover_cost_units: descriptor.costs.load_units,
                    });
                    plan.routes.push(ExecutionRoute::Local {
                        key,
                        model: Arc::clone(&model),
                        backend: Arc::clone(&backend),
                        device: device.id.clone(),
                    });
                }
            }
        }
        Ok(plan)
    }
}

fn merge_metrics(backend: PlacementMetrics, hardware: PlacementMetrics) -> PlacementMetrics {
    PlacementMetrics {
        load_percent: hardware.load_percent.or(backend.load_percent),
        queue_depth: backend.queue_depth,
        available_memory_bytes: hardware
            .available_memory_bytes
            .or(backend.available_memory_bytes),
        available_vram_bytes: hardware
            .available_vram_bytes
            .or(backend.available_vram_bytes),
        latency_ema_ms: backend.latency_ema_ms,
        throughput_ema: backend.throughput_ema,
    }
}
