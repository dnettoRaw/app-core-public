// =============================================================================
//        #######
//     ###       ###     F: stress_soak.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/22 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/22 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use appcore_ai::*;
use std::future::Future;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

const DEFAULT_SOAK_ITERATIONS: usize = 20_000;
const MAX_SOAK_ITERATIONS: usize = 1_000_000;

#[test]
fn lightweight_soak_keeps_queues_and_model_load_state_empty() {
    let iterations = std::env::var("APPCORE_AI_SOAK_ITERATIONS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(DEFAULT_SOAK_ITERATIONS)
        .min(MAX_SOAK_ITERATIONS);
    let limits = AiLimits::default();
    let runtime = AiRuntime::new(
        limits,
        Arc::new(LightweightEngine::new(Vec::new(), limits, 8_000).unwrap()),
        Arc::new(ModelRegistry::new()),
        Arc::new(BackendRegistry::new()),
        Arc::new(UnusedAdmission),
    )
    .unwrap();
    for _ in 0..iterations {
        let request = AiRequest::text(AiTask::TransformText, " bounded   input ", limits).unwrap();
        let response = block_on(runtime.resolve(request)).unwrap();
        assert_eq!(response.output, AiOutput::Text("bounded input".into()));
    }
    let telemetry = runtime.telemetry();
    assert_eq!(telemetry.requests, u64::try_from(iterations).unwrap());
    assert_eq!(telemetry.successes, u64::try_from(iterations).unwrap());
    assert_eq!(telemetry.failures, 0);
    assert_eq!(runtime.execution_queue().active, 0);
    assert_eq!(runtime.execution_queue().queued, 0);
    assert_eq!(runtime.model_loads(), ModelLoadSnapshot::default());
}

#[derive(Debug)]
struct UnusedAdmission;

impl ModelAdmission for UnusedAdmission {
    fn admit(
        &self,
        _request: &AiRequest,
        _estimate: ResourceEstimate,
    ) -> AiResult<AdmissionDecision> {
        Err(AiError::Capacity("soak backend path is unused"))
    }
}

fn block_on<T>(future: impl Future<Output = T>) -> T {
    let mut future = Box::pin(future);
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}
