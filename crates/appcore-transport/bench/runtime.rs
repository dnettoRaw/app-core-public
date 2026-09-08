// =============================================================================
//        #######
//     ###       ###     F: runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/31 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/31 12:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Measures bounded gzip encoding and shared HTTP request clones.

use appcore_transport::{parse_response, parse_response_owned, HttpRequest};
use std::hint::black_box;
use std::time::Instant;

const GZIP_CASE: &str = "gzip_4k";
const GZIP_INCOMPRESSIBLE_CASE: &str = "gzip_incompressible_4mib";
const IDENTITY_RESPONSE_CASE: &str = "identity_response_4mib";
const OWNED_IDENTITY_RESPONSE_CASE: &str = "identity_response_owned_4mib";
const CHUNKED_RESPONSE_CASE: &str = "chunked_response_4mib";
const OWNED_CHUNKED_RESPONSE_CASE: &str = "chunked_response_owned_4mib";
const SHARED_REQUEST_CASE: &str = "http_request_clone_1mib_shared";
const LARGE_BYTES: usize = 4 * 1024 * 1024;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    memory_checkpoint("idle", true);
    let selected = std::env::var("APPCORE_BENCH_CASE").ok();
    if selected.as_deref().is_none_or(|value| value == GZIP_CASE) {
        benchmark_gzip();
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == GZIP_INCOMPRESSIBLE_CASE)
    {
        benchmark_gzip_incompressible();
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == IDENTITY_RESPONSE_CASE)
    {
        benchmark_identity_response();
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == OWNED_IDENTITY_RESPONSE_CASE)
    {
        benchmark_owned_identity_response();
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == CHUNKED_RESPONSE_CASE)
    {
        benchmark_chunked_response();
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == OWNED_CHUNKED_RESPONSE_CASE)
    {
        benchmark_owned_chunked_response();
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == SHARED_REQUEST_CASE)
    {
        benchmark_shared_request()?;
    }
    if let Some(value) = selected.as_deref() {
        if ![
            GZIP_CASE,
            GZIP_INCOMPRESSIBLE_CASE,
            IDENTITY_RESPONSE_CASE,
            OWNED_IDENTITY_RESPONSE_CASE,
            CHUNKED_RESPONSE_CASE,
            OWNED_CHUNKED_RESPONSE_CASE,
            SHARED_REQUEST_CASE,
        ]
        .contains(&value)
        {
            return Err(format!("unknown appcore-transport benchmark case: {value}").into());
        }
    }
    memory_checkpoint("retained", true);
    Ok(())
}

fn benchmark_gzip_incompressible() {
    let mut state = 0x1234_5678_u32;
    let input: Vec<u8> = (0..LARGE_BYTES)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect();
    measure(GZIP_INCOMPRESSIBLE_CASE, 10, || {
        let encoded = appcore_transport::encode_gzip_if_smaller(black_box(&input))
            .expect("bounded gzip candidate");
        assert!(encoded.is_none());
    });
}

fn benchmark_identity_response() {
    measure(IDENTITY_RESPONSE_CASE, 100, || {
        let raw = identity_response_frame();
        let response =
            parse_response(black_box(&raw), 1024, LARGE_BYTES).expect("bounded identity response");
        assert_eq!(response.body.len(), LARGE_BYTES);
        black_box(response);
    });
}

fn benchmark_owned_identity_response() {
    measure(OWNED_IDENTITY_RESPONSE_CASE, 100, || {
        let raw = identity_response_frame();
        let response = parse_response_owned(black_box(raw), 1024, LARGE_BYTES)
            .expect("bounded owned identity response");
        assert_eq!(response.body.len(), LARGE_BYTES);
        black_box(response);
    });
}

fn identity_response_frame() -> Vec<u8> {
    let mut raw = format!("HTTP/1.1 200 OK\r\nContent-Length: {LARGE_BYTES}\r\n\r\n").into_bytes();
    raw.resize(raw.len() + LARGE_BYTES, 0x5a);
    raw
}

fn benchmark_chunked_response() {
    measure(CHUNKED_RESPONSE_CASE, 100, || {
        let raw = chunked_response_frame();
        let response =
            parse_response(black_box(&raw), 1024, LARGE_BYTES).expect("bounded chunked response");
        assert_eq!(response.body.len(), LARGE_BYTES);
        black_box(response);
    });
}

fn benchmark_owned_chunked_response() {
    measure(OWNED_CHUNKED_RESPONSE_CASE, 100, || {
        let raw = chunked_response_frame();
        let response = parse_response_owned(black_box(raw), 1024, LARGE_BYTES)
            .expect("bounded owned chunked response");
        assert_eq!(response.body.len(), LARGE_BYTES);
        black_box(response);
    });
}

fn chunked_response_frame() -> Vec<u8> {
    const CHUNK_BYTES: usize = 64 * 1024;
    let mut raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
    for _ in 0..LARGE_BYTES / CHUNK_BYTES {
        raw.extend_from_slice(b"10000\r\n");
        raw.resize(raw.len() + CHUNK_BYTES, 0x5a);
        raw.extend_from_slice(b"\r\n");
    }
    raw.extend_from_slice(b"0\r\n\r\n");
    raw
}

fn benchmark_gzip() {
    let input = [b'a'; 4_096];
    measure(GZIP_CASE, 5_000, || {
        let _ = black_box(appcore_transport::encode_gzip_if_smaller(black_box(&input)));
    });
}

fn benchmark_shared_request() -> Result<(), Box<dyn std::error::Error>> {
    let request = HttpRequest::new("POST", vec![0x5a; 1024 * 1024])?;
    measure(SHARED_REQUEST_CASE, 100_000, || {
        black_box(request.clone());
    });
    Ok(())
}

fn iterations(fallback: u64) -> u64 {
    std::env::var("APPCORE_BENCH_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}

fn measure(case_name: &str, fallback_iterations: u64, mut operation: impl FnMut()) {
    let iterations = iterations(fallback_iterations);
    memory_checkpoint("workload", true);
    let started = Instant::now();
    for _ in 0..iterations {
        operation();
    }
    let total_ns = started.elapsed().as_nanos();
    println!(
        "appcore-transport::{case_name} iterations={iterations} total_ns={total_ns} ns_per_iter={:.2}",
        total_ns as f64 / iterations as f64
    );
}

fn memory_checkpoint(phase: &str, settle: bool) {
    let Some(milliseconds) = checkpoint_milliseconds() else {
        return;
    };
    println!(
        "appcore-bench-memory phase={phase} pid={}",
        std::process::id()
    );
    let _ = std::io::Write::flush(&mut std::io::stdout());
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
