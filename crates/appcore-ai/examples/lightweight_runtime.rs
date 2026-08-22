// =============================================================================
//        #######
//     ###       ###     F: lightweight_runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use appcore_ai::{
    AiContributionPolicy, AiExecutionMode, AiLimits, AiOutput, AiPrivacyMode, AiRequest, AiRuntime,
    AiTask, BackendRegistry, GovernorAdmission, LightweightEngine, ModelRegistry, ResourceGovernor,
    ResourceGovernorConfig, RuleMatch, SystemAiClock, SystemHardwareProbe, TextRule,
};
use std::future::Future;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let limits = AiLimits {
        max_input_bytes: 256,
        max_output_bytes: 256,
        ..AiLimits::default()
    };
    let lightweight = LightweightEngine::new(
        vec![TextRule {
            label: "service.status".into(),
            pattern: "status".into(),
            output: "operational".into(),
            matching: RuleMatch::Exact,
        }],
        limits,
        8_000,
    )?;
    let governor = ResourceGovernor::new(
        SystemHardwareProbe::default(),
        ResourceGovernorConfig::default(),
        AiContributionPolicy::default(),
    )?;
    let admission = GovernorAdmission::new(governor, SystemAiClock::new());
    let runtime = AiRuntime::new(
        limits,
        Arc::new(lightweight),
        Arc::new(ModelRegistry::new()),
        Arc::new(BackendRegistry::new()),
        Arc::new(admission),
    )?;

    let mut request = AiRequest::text(AiTask::ClassifyText, "status", limits)?;
    request.options.execution = AiExecutionMode::Local;
    request.options.privacy = AiPrivacyMode::LocalOnly;
    request.options.include_diagnostics = true;

    let response = block_on(runtime.resolve(request))?;
    if let AiOutput::Scores(scores) = response.output {
        for score in scores {
            println!("label={} score={:.3}", score.label, score.score);
        }
    }
    if let Some(decision) = response.decision {
        println!(
            "route={:?} attempts={}",
            decision.selected,
            decision.attempts.len()
        );
    }
    let telemetry = runtime.telemetry();
    println!(
        "requests={} successes={} lightweight={}",
        telemetry.requests, telemetry.successes, telemetry.lightweight_placements
    );
    Ok(())
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
