// =============================================================================
//        #######
//     ###       ###     F: application_ai_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-alpha
// =============================================================================

use super::*;
use appcore_ai::{
    AdmissionDecision, AiLimits, AiOutput, AiResult, BackendRegistry, LightweightEngine,
    ModelAdmission, ModelRegistry, ResourceBudget, ResourceEstimate,
};

#[derive(Debug)]
struct AllowAdmission;

impl ModelAdmission for AllowAdmission {
    fn admit(
        &self,
        _request: &AiRequest,
        _estimate: ResourceEstimate,
    ) -> AiResult<AdmissionDecision> {
        Ok(AdmissionDecision::Admit {
            budget: ResourceBudget {
                cpu_percent: 100,
                gpu_percent: 100,
                memory_bytes: None,
                vram_bytes: None,
                storage_bytes: 0,
                workers: 1,
                concurrent_jobs: 1,
                pressure_limited: false,
            },
        })
    }
}

#[derive(Debug)]
struct TextCodec;

impl AiCapabilityCodec for TextCodec {
    fn decode_request(&self, payload: &[u8]) -> Result<AiRequest, String> {
        let text = std::str::from_utf8(payload).map_err(|_| "UTF-8".to_string())?;
        AiRequest::text(appcore_ai::AiTask::TransformText, text, AiLimits::default())
            .map_err(|error| error.to_string())
    }

    fn encode_response(&self, response: &AiResponse) -> Result<Vec<u8>, String> {
        match &response.output {
            AiOutput::Text(text) => Ok(text.as_bytes().to_vec()),
            _ => Err("unexpected output".to_string()),
        }
    }
}

#[test]
fn optional_component_registers_and_executes_canonical_capability() {
    let component = AppCoreAiComponent::new(runtime(), false).unwrap();
    let service = component.managed_service();
    service.start().unwrap();
    assert_eq!(service.health(), ServiceHealth::Degraded);

    let mut registry = CapabilityRegistry::new();
    component
        .register_capability(&mut registry, Arc::new(TextCodec))
        .unwrap();
    let name = CapabilityName::new(AI_RESOLVE_CAPABILITY).unwrap();
    let provider = registry.get(&name).unwrap();
    let response = provider
        .handle(&CapabilityRequest {
            request_id: "request-1".to_string(),
            capability: name,
            mode: CapabilityMode::Query,
            payload: b"  bounded   response  ".to_vec(),
            idempotency_key: None,
            trace: None,
        })
        .unwrap();
    assert!(response.accepted);
    assert_eq!(response.payload, b"bounded response");
    service.stop(Duration::from_secs(1)).unwrap();
    assert!(matches!(
        block_on(
            component.facade().resolve(
                AiRequest::text(
                    appcore_ai::AiTask::TransformText,
                    "after stop",
                    AiLimits::default(),
                )
                .unwrap(),
            )
        ),
        Err(AiError::BackendUnavailable(_))
    ));
}

#[test]
fn required_component_fails_closed_without_backend_and_model() {
    let component = AppCoreAiComponent::new(runtime(), true).unwrap();
    assert!(matches!(
        component.managed_service().start(),
        Err(appcore_supervisor::SupervisorError::ServiceFailure { .. })
    ));
}

fn runtime() -> Arc<AiRuntime> {
    Arc::new(
        AiRuntime::new(
            AiLimits::default(),
            Arc::new(LightweightEngine::new(Vec::new(), AiLimits::default(), 10_000).unwrap()),
            Arc::new(ModelRegistry::new()),
            Arc::new(BackendRegistry::new()),
            Arc::new(AllowAdmission),
        )
        .unwrap(),
    )
}
