// =============================================================================
//        #######
//     ###       ###     F: runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/31 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/02 20:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Measures tool-contract construction and bounded JSON result sizing.

use appcore_filemaker::{Compiler, DataValue, FontManager, ResourceLimits};
use appcore_filemaker_ai::{AiBridgePolicy, FileMakerAiSession};
use std::collections::{BTreeMap, BTreeSet};
use std::hint::black_box;
use std::io::Write;
use std::time::Instant;

const TOOL_DEFINITIONS_CASE: &str = "tool_definitions";
const RESULT_LIMIT_CASE: &str = "capabilities_result_limit_20k_ids";
const RESULT_TEMPLATE: &[u8] = br"filemaker: '1.0'
model: canvas
id: ai-result-benchmark
page: { width: 40pt, height: 40pt }
elements:
  - { id: box, type: rect, width: 10pt, height: 10pt }
";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    memory_checkpoint("idle", true);
    let selected = std::env::var("APPCORE_BENCH_CASE").ok();
    if selected
        .as_deref()
        .is_none_or(|value| value == TOOL_DEFINITIONS_CASE)
    {
        benchmark_tool_definitions()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == RESULT_LIMIT_CASE)
    {
        benchmark_result_limit()?;
    }
    if let Some(value) = selected.as_deref() {
        if value != TOOL_DEFINITIONS_CASE && value != RESULT_LIMIT_CASE {
            return Err(format!("unknown FileMaker AI benchmark case: {value}").into());
        }
    }
    memory_checkpoint("retained", true);
    Ok(())
}

fn benchmark_tool_definitions() -> Result<(), Box<dyn std::error::Error>> {
    measure(TOOL_DEFINITIONS_CASE, 10_000, || {
        black_box(appcore_filemaker_ai::tool_definitions());
        Ok(())
    })
}

fn benchmark_result_limit() -> Result<(), Box<dyn std::error::Error>> {
    let compiler = Compiler::builder().build()?;
    let template = compiler.compile_template_yaml(RESULT_TEMPLATE)?;
    let mut document = compiler.bind(&template, &DataValue::Object(BTreeMap::new()), &[])?;
    document.ai_policy.purpose = "p".repeat(1_024);
    document.ai_policy.rules = (0..64)
        .map(|index| format!("rule-{index:02}-{}", "r".repeat(1_000)))
        .collect();
    document.ai_policy.editable = policy_ids("editable");
    document.ai_policy.locked = policy_ids("locked");
    let mut session = FileMakerAiSession::new(
        document,
        ResourceLimits::default(),
        FontManager::default(),
        None,
        AiBridgePolicy {
            max_tool_calls: 1_000_000,
            ..AiBridgePolicy::default()
        },
    )?;
    measure(RESULT_LIMIT_CASE, 25, || {
        black_box(session.execute("filemaker_capabilities", "{}")?);
        Ok(())
    })
}

fn policy_ids(prefix: &str) -> BTreeSet<String> {
    (0..10_000)
        .map(|index| format!("{prefix}-{index:05}"))
        .collect()
}

fn measure(
    case_name: &str,
    fallback_iterations: u64,
    mut operation: impl FnMut() -> Result<(), Box<dyn std::error::Error>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let iterations = iterations(fallback_iterations);
    memory_checkpoint("workload", true);
    let started = Instant::now();
    for _ in 0..iterations {
        operation()?;
    }
    let total_ns = started.elapsed().as_nanos();
    println!(
        "appcore-filemaker-ai::{case_name} iterations={iterations} total_ns={total_ns} ns_per_iter={:.2}",
        total_ns as f64 / iterations as f64
    );
    Ok(())
}

fn iterations(fallback: u64) -> u64 {
    std::env::var("APPCORE_BENCH_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}

fn memory_checkpoint(phase: &str, settle: bool) {
    let Some(milliseconds) = checkpoint_milliseconds() else {
        return;
    };
    println!(
        "appcore-bench-memory phase={phase} pid={}",
        std::process::id()
    );
    let _ = std::io::stdout().flush();
    if settle {
        std::thread::sleep(std::time::Duration::from_millis(milliseconds));
    }
}

fn checkpoint_milliseconds() -> Option<u64> {
    std::env::var("APPCORE_BENCH_MEMORY_CHECKPOINT_MS")
        .ok()?
        .parse()
        .ok()
        .filter(|value| (1..=1_000).contains(value))
}
