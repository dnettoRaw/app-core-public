// =============================================================================
//        #######
//     ###       ###     F: runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/31 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/31 12:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Measures authenticated DNT header inspection.

use appcore_contracts::ApplicationId;
use appcore_dnt::{
    seal, BytesCodec, ContentType, DntSealOptions, KeyId, SecretKey, StaticDntKeyProvider,
};
use std::hint::black_box;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    memory_checkpoint("idle");
    let key_id = KeyId::new("bench-key")?;
    let mut ephemeral_key = [0_u8; 32];
    getrandom::fill(&mut ephemeral_key)
        .map_err(|_| std::io::Error::other("benchmark key generation failed"))?;
    let provider =
        StaticDntKeyProvider::new().with_key(key_id.clone(), SecretKey::new(ephemeral_key));
    let envelope = seal(
        &[0x5a; 1_024],
        &provider,
        &BytesCodec,
        DntSealOptions {
            application_id: ApplicationId::new("bench.application")?,
            tenant_id: None,
            content_type: ContentType::new("bench.payload")?,
            schema_version: 1,
            key_id,
            created_at_ms: 1,
            public_metadata: Vec::new(),
            encrypted_metadata: Vec::new(),
            flags: 0,
            max_payload_bytes: Some(2_048),
        },
    )?;
    let iterations = iterations(50_000);
    let started = benchmark_started();
    for _ in 0..iterations {
        let _ = black_box(appcore_dnt::inspect_header(&envelope));
    }
    report(iterations, started.elapsed().as_nanos());
    Ok(())
}

fn iterations(fallback: u64) -> u64 {
    std::env::var("APPCORE_BENCH_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}

fn report(iterations: u64, total_ns: u128) {
    println!(
        "appcore-dnt::inspect_header_1k iterations={iterations} total_ns={total_ns} ns_per_iter={:.2}",
        total_ns as f64 / iterations as f64
    );
    memory_checkpoint("retained");
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
