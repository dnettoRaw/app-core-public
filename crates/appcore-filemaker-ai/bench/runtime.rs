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
const EDIT_CASE: &str = "create_patch_256_elements";
const EXPLAIN_CASE: &str = "explain_layout_256_elements";
const STREAMED_EXPORT_CASE: &str = "export_svg_stream_base64_20000";
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
        if value != TOOL_DEFINITIONS_CASE
            && value != RESULT_LIMIT_CASE
            && value != EDIT_CASE
            && value != EXPLAIN_CASE
            && value != STREAMED_EXPORT_CASE
        {
            return Err(format!("unknown FileMaker AI benchmark case: {value}").into());
        }
    }
    if selected.as_deref().is_none_or(|value| value == EDIT_CASE) {
        benchmark_edit()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == EXPLAIN_CASE)
    {
        benchmark_explain()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == STREAMED_EXPORT_CASE)
    {
        benchmark_streamed_export()?;
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

fn benchmark_edit() -> Result<(), Box<dyn std::error::Error>> {
    use std::fmt::Write as _;
    let mut yaml = String::from(
        "filemaker: '1.0'\nmodel: canvas\nid: edit-benchmark\npage: { width: 500pt, height: 500pt }\nelements:\n",
    );
    for index in 0..256 {
        writeln!(
            yaml,
            "  - {{ id: box-{index:04}, type: rect, x: {}pt, y: {}pt, width: 20pt, height: 20pt }}",
            index % 16 * 30,
            index / 16 * 30
        )?;
    }
    let compiler = Compiler::builder().build()?;
    let template = compiler.compile_template_yaml(yaml.as_bytes())?;
    let document = compiler.bind(&template, &DataValue::Object(BTreeMap::new()), &[])?;
    assert_eq!(document.elements.len(), 256);
    let arguments = serde_json::json!({"document":document}).to_string();
    assert!(arguments.len() <= 1024 * 1024);
    // Compile/bind and request construction are fixtures, outside the timer.
    drop((compiler, template, document, yaml));
    measure(EDIT_CASE, 25, || {
        let mut session = FileMakerAiSession::empty(
            ResourceLimits::default(),
            FontManager::default(),
            None,
            AiBridgePolicy {
                max_argument_bytes: 1024 * 1024,
                ..AiBridgePolicy::default()
            },
        )?;
        let created = session.execute("filemaker_create", &arguments)?;
        assert_eq!(created.revision, 1);
        assert_eq!(created.value["pages"], 1);
        let edited = session.execute("filemaker_set", r#"{"id":"box-0000","hidden":true}"#)?;
        assert_eq!(edited.revision, 2);
        assert_eq!(edited.value["applied_operations"], 1);
        let inspected = session.execute("filemaker_inspect", r#"{"id":"box-0001"}"#)?;
        assert_eq!(inspected.revision, 2);
        black_box(inspected);
        Ok(())
    })
}

fn benchmark_explain() -> Result<(), Box<dyn std::error::Error>> {
    use std::fmt::Write as _;
    let mut yaml = String::from(
        "filemaker: '1.0'\nmodel: canvas\nid: explain-benchmark\npage: { width: 500pt, height: 500pt }\nelements:\n",
    );
    for index in 0..256 {
        writeln!(
            yaml,
            "  - {{ id: box-{index:04}, type: rect, x: {}pt, y: {}pt, width: 20pt, height: 20pt }}",
            index % 16 * 30,
            index / 16 * 30
        )?;
    }
    let compiler = Compiler::builder().build()?;
    let template = compiler.compile_template_yaml(yaml.as_bytes())?;
    let document = compiler.bind(&template, &DataValue::Object(BTreeMap::new()), &[])?;
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
    let arguments = r#"{"id":"box-0255"}"#;
    measure(EXPLAIN_CASE, 1_000, || {
        let result = session.execute("filemaker_explain", arguments)?;
        assert_eq!(result.value["id"], "box-0255");
        black_box(result);
        Ok(())
    })
}

fn benchmark_streamed_export() -> Result<(), Box<dyn std::error::Error>> {
    use std::fmt::Write as _;
    let mut yaml = String::from(
        "filemaker: '1.0'\nmodel: canvas\nid: export-benchmark\npage: { width: 1000pt, height: 1000pt }\ncollision: false\nelements:\n",
    );
    for index in 0..20_000 {
        writeln!(
            yaml,
            "  - {{ id: box-{index:05}, type: rect, x: {}pt, y: {}pt, width: 4pt, height: 4pt, style: {{ fill: '#336699' }} }}",
            index % 250 * 4,
            index / 250 * 4
        )?;
    }
    let compiler = Compiler::builder().build()?;
    let template = compiler.compile_template_yaml(yaml.as_bytes())?;
    let document = compiler.bind(&template, &DataValue::Object(BTreeMap::new()), &[])?;
    let mut session = FileMakerAiSession::new(
        document,
        ResourceLimits::default(),
        FontManager::default(),
        None,
        AiBridgePolicy {
            max_tool_calls: 1_000_000,
            max_result_bytes: 64 * 1024 * 1024,
            ..AiBridgePolicy::default()
        },
    )?;
    drop((compiler, template, yaml));
    let mut raw_bytes = 0_u64;
    let mut encoded_bytes = 0_usize;
    measure(STREAMED_EXPORT_CASE, 3, || {
        let result = session.execute(
            "filemaker_export",
            r#"{"format":"svg","fidelity":"best_effort"}"#,
        )?;
        raw_bytes = result.value["bytes"].as_u64().unwrap_or_default();
        encoded_bytes = result.value["base64"]
            .as_str()
            .map(str::len)
            .unwrap_or_default();
        assert!(raw_bytes > 1_000_000);
        black_box(result);
        Ok(())
    })?;
    println!(
        "appcore-filemaker-ai::{STREAMED_EXPORT_CASE} raw_bytes={raw_bytes} encoded_bytes={encoded_bytes}"
    );
    Ok(())
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
