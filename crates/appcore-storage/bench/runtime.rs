// =============================================================================
//        #######
//     ###       ###     F: runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/31 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/31 12:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Measures capability construction and maximum default remote-auth roundtrip.

use appcore_security::HashTokenProvider;
use appcore_storage::{
    make_auth_request, now_ms, open_remote_request, open_remote_response, process_remote_request,
    seal_remote_request, seal_remote_response, DEFAULT_AUTH_REMOTE_MAX_PLAINTEXT_BYTES,
};
use std::hint::black_box;
use std::io::Write;
use std::time::Instant;

const CAPABILITY_CASE: &str = "file_capability_descriptor";
const AUTH_REMOTE_CASE: &str = "auth_remote_roundtrip_256kib";

fn main() -> Result<(), String> {
    memory_checkpoint("idle", true);
    let selected = std::env::var("APPCORE_BENCH_CASE").ok();
    if selected
        .as_deref()
        .is_none_or(|value| value == CAPABILITY_CASE)
    {
        benchmark_capability()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == AUTH_REMOTE_CASE)
    {
        benchmark_auth_remote()?;
    }
    if let Some(value) = selected.as_deref() {
        if value != CAPABILITY_CASE && value != AUTH_REMOTE_CASE {
            return Err(format!("unknown appcore-storage benchmark case: {value}"));
        }
    }
    memory_checkpoint("retained", true);
    Ok(())
}

fn benchmark_capability() -> Result<(), String> {
    measure(CAPABILITY_CASE, 100_000, || {
        let _ = black_box(appcore_storage::file_storage_capability_descriptor_v1());
        Ok(())
    })
}

fn benchmark_auth_remote() -> Result<(), String> {
    let transport = bench_result(HashTokenProvider::from_secret(
        b"transport-secret-1234567890".to_vec(),
    ))?;
    let data = bench_result(HashTokenProvider::from_secret(
        b"data-secret-123456789012345".to_vec(),
    ))?;
    let plaintext = vec![0x5a; DEFAULT_AUTH_REMOTE_MAX_PLAINTEXT_BYTES];
    measure(AUTH_REMOTE_CASE, 1, || {
        let timestamp = now_ms();
        let seal_request = bench_result(make_auth_request(
            "private.bin",
            "seal",
            &plaintext,
            timestamp,
        ))?;
        let seal_token = bench_result(seal_remote_request(&seal_request, &transport))?;
        let opened_seal = bench_result(open_remote_request(&seal_token, &transport, timestamp))?;
        drop(seal_token);
        let seal_response = bench_result(process_remote_request(&opened_seal, &data))?;
        drop(opened_seal);
        let seal_response_token = bench_result(seal_remote_response(&seal_response, &transport))?;
        drop(seal_response);
        let sealed = bench_result(open_remote_response(
            &seal_response_token,
            &transport,
            &seal_request.nonce,
            timestamp,
        ))?;
        drop(seal_response_token);
        drop(seal_request);
        let open_request =
            bench_result(make_auth_request("private.bin", "open", &sealed, timestamp))?;
        let open_token = bench_result(seal_remote_request(&open_request, &transport))?;
        let opened_open = bench_result(open_remote_request(&open_token, &transport, timestamp))?;
        drop(open_token);
        let open_response = bench_result(process_remote_request(&opened_open, &data))?;
        drop(opened_open);
        let open_response_token = bench_result(seal_remote_response(&open_response, &transport))?;
        drop(open_response);
        let recovered = bench_result(open_remote_response(
            &open_response_token,
            &transport,
            &open_request.nonce,
            timestamp,
        ))?;
        drop(open_response_token);
        drop(open_request);
        drop(sealed);
        if recovered != plaintext {
            return Err("remote-auth benchmark roundtrip mismatch".to_string());
        }
        black_box(recovered);
        Ok(())
    })
}

fn iterations(fallback: u64) -> u64 {
    std::env::var("APPCORE_BENCH_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}

fn measure(
    case_name: &str,
    fallback_iterations: u64,
    mut operation: impl FnMut() -> Result<(), String>,
) -> Result<(), String> {
    let iterations = iterations(fallback_iterations);
    memory_checkpoint("workload", true);
    let started = Instant::now();
    for _ in 0..iterations {
        operation()?;
    }
    let total_ns = started.elapsed().as_nanos();
    println!(
        "appcore-storage::{case_name} iterations={iterations} total_ns={total_ns} ns_per_iter={:.2}",
        total_ns as f64 / iterations as f64
    );
    Ok(())
}

fn bench_result<T, E: std::fmt::Debug>(result: Result<T, E>) -> Result<T, String> {
    result.map_err(|error| format!("{error:?}"))
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
