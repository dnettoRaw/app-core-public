// =============================================================================
//        #######
//     ###       ###     F: runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/31 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/31 12:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Measures diagnostic redaction and request-bound hashing ownership.

use appcore_security::{RequestPayloadRef, RequestValidationDetails, RequestValidationDetailsRef};
use std::hint::black_box;
use std::time::Instant;

const REDACTION_CASE: &str = "redact_64b";
const OWNED_HASH_CASE: &str = "request_hash_owned_json_4mib";
const BORROWED_HASH_CASE: &str = "request_hash_borrowed_json_4mib";
const PAYLOAD_BYTES: usize = 4 * 1_024 * 1_024;
const CASES: &[&str] = &[REDACTION_CASE, OWNED_HASH_CASE, BORROWED_HASH_CASE];

fn main() -> Result<(), String> {
    memory_checkpoint("idle");
    let selected = std::env::var("APPCORE_BENCH_CASE").ok();
    if selected
        .as_deref()
        .is_none_or(|value| value == REDACTION_CASE)
    {
        benchmark_redaction();
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == OWNED_HASH_CASE)
    {
        benchmark_owned_request_hash();
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == BORROWED_HASH_CASE)
    {
        benchmark_borrowed_request_hash();
    }
    if let Some(value) = selected.as_deref() {
        if !CASES.contains(&value) {
            return Err(format!("unknown appcore-security benchmark case: {value}"));
        }
    }
    memory_checkpoint("retained");
    Ok(())
}

fn benchmark_redaction() {
    let input = "authorization=Bearer <redacted> password=<redacted> operation=bench";
    measure(REDACTION_CASE, 100_000, || {
        black_box(appcore_security::redact_text(black_box(input)));
    });
}

fn benchmark_owned_request_hash() {
    let payload = payload_fixture();
    measure(OWNED_HASH_CASE, 10, || {
        let details = RequestValidationDetails {
            purpose: "query".to_string(),
            name: "runtime.status".to_string(),
            id: "query-benchmark".to_string(),
            idempotency_key: None,
            payload: serde_json::to_string(&payload).unwrap_or_default(),
            subject: None,
            audience: None,
        };
        black_box(appcore_security::compute_request_hash(&details));
    });
}

fn benchmark_borrowed_request_hash() {
    let payload = payload_fixture();
    let details = RequestValidationDetailsRef {
        purpose: "query",
        name: "runtime.status",
        id: "query-benchmark",
        idempotency_key: None,
        payload: RequestPayloadRef::Json(&payload),
        subject: None,
        audience: None,
    };
    measure(BORROWED_HASH_CASE, 10, || {
        let _ = black_box(appcore_security::compute_borrowed_request_hash(&details));
    });
}

fn payload_fixture() -> serde_json::Value {
    serde_json::json!({"data": "x".repeat(PAYLOAD_BYTES)})
}

fn iterations(fallback: u64) -> u64 {
    std::env::var("APPCORE_BENCH_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}

fn measure(case: &str, fallback: u64, mut operation: impl FnMut()) {
    let iterations = iterations(fallback);
    let started = benchmark_started();
    for _ in 0..iterations {
        operation();
    }
    let total_ns = started.elapsed().as_nanos();
    println!(
        "appcore-security::{case} iterations={iterations} total_ns={total_ns} ns_per_iter={:.2}",
        total_ns as f64 / iterations as f64
    );
}

fn benchmark_started() -> Instant {
    memory_checkpoint("workload");
    Instant::now()
}
fn memory_checkpoint(phase: &str) {
    let Some(milliseconds) = checkpoint_milliseconds() else {
        return;
    };
    println!(
        "appcore-bench-memory phase={phase} pid={}",
        std::process::id()
    );
    let _ = std::io::Write::flush(&mut std::io::stdout());
    std::thread::sleep(std::time::Duration::from_millis(milliseconds));
}
fn checkpoint_milliseconds() -> Option<u64> {
    std::env::var("APPCORE_BENCH_MEMORY_CHECKPOINT_MS")
        .ok()?
        .parse()
        .ok()
        .filter(|value| (1..=1_000).contains(value))
}
