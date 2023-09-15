// =============================================================================
//        #######
//     ###       ###     F: compare.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/02 10:29:16 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 13:38:21 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: allow-file(clippy::expect_used) reason: example exits immediately when a required invariant is violated

use appcore_contracts::ApplicationId;
use appcore_dnt::{
    open_owned, rekey, seal, BytesCodec, ContentType, DntOpenOptions, DntSealOptions, KeyId,
    SecretKey, StaticDntKeyProvider, DNT_CONTENT_JSON, DNT_CONTENT_OCTET_STREAM,
};
use appcore_types::TenantId;
use std::error::Error;
use std::fs;
use std::hint::black_box;
use std::path::Path;
use std::process::{self, Command};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MAX_PAYLOAD_BYTES: u64 = 8 * 1024 * 1024;
const WARM_UP_ITERATIONS: usize = 20;
const READ_ITERATIONS: usize = 200;
const CRYPTO_ITERATIONS: usize = 100;

type BenchResult<T> = Result<T, Box<dyn Error>>;

fn main() -> BenchResult<()> {
    let key_id = KeyId::new("bench-key")?;
    let rotated_key_id = KeyId::new("bench-key-rotated")?;
    let key_provider = StaticDntKeyProvider::new()
        .with_key(key_id.clone(), SecretKey::new([7u8; 32]))
        .with_key(rotated_key_id.clone(), SecretKey::new([11u8; 32]));
    let codec = BytesCodec;
    let temp_root = benchmark_temp_root()?;
    fs::create_dir_all(&temp_root)?;

    print_environment(&temp_root);

    let samples = [
        Sample::new(
            "Repetitive JSON snapshot",
            "json-snapshot",
            DNT_CONTENT_JSON,
            json_snapshot(1024 * 1024),
        ),
        Sample::new(
            "Deterministic binary",
            "binary",
            DNT_CONTENT_OCTET_STREAM,
            deterministic_binary(1024 * 1024),
        ),
        Sample::new(
            "Small local secret",
            "local-secret",
            "appcore/secret",
            b"key_id=local-dev\nsecret_ref=provider:dnt-key/bench\nstatus=active\n".to_vec(),
        ),
    ];

    for sample in samples {
        compare_sample(
            &temp_root,
            &sample,
            &key_id,
            &rotated_key_id,
            &key_provider,
            &codec,
        )?;
    }

    fs::remove_dir_all(&temp_root)?;
    Ok(())
}

fn compare_sample(
    temp_root: &Path,
    sample: &Sample,
    key_id: &KeyId,
    rotated_key_id: &KeyId,
    key_provider: &StaticDntKeyProvider,
    codec: &BytesCodec,
) -> BenchResult<()> {
    let normal_options = seal_options(key_id, sample.content_type);
    let compact_options = seal_options(key_id, sample.content_type).compact_payload();
    let open_options = open_options(sample.content_type);
    let normal = seal(&sample.payload, key_provider, codec, normal_options.clone())?;
    let compact = seal(
        &sample.payload,
        key_provider,
        codec,
        compact_options.clone(),
    )?;

    let plain_path = temp_root.join(format!("{}.plain", sample.slug));
    let normal_path = temp_root.join(format!("{}.dnt", sample.slug));
    let compact_path = temp_root.join(format!("{}.compact.dnt", sample.slug));
    fs::write(&plain_path, &sample.payload)?;
    fs::write(&normal_path, &normal)?;
    fs::write(&compact_path, &compact)?;

    let plain_read = benchmark(READ_ITERATIONS, sample.payload.len(), || {
        let bytes = fs::read(&plain_path)?;
        black_box(bytes);
        Ok(())
    })?;
    let normal_open = benchmark(READ_ITERATIONS, sample.payload.len(), || {
        let envelope = fs::read(&normal_path)?;
        let opened = open_owned(envelope, key_provider, codec, &open_options)?;
        black_box(opened);
        Ok(())
    })?;
    let compact_open = benchmark(READ_ITERATIONS, sample.payload.len(), || {
        let envelope = fs::read(&compact_path)?;
        let opened = open_owned(envelope, key_provider, codec, &open_options)?;
        black_box(opened);
        Ok(())
    })?;
    let normal_seal = benchmark(CRYPTO_ITERATIONS, sample.payload.len(), || {
        let envelope = seal(
            black_box(&sample.payload),
            key_provider,
            codec,
            normal_options.clone(),
        )?;
        black_box(envelope);
        Ok(())
    })?;
    let compact_seal = benchmark(CRYPTO_ITERATIONS, sample.payload.len(), || {
        let envelope = seal(
            black_box(&sample.payload),
            key_provider,
            codec,
            compact_options.clone(),
        )?;
        black_box(envelope);
        Ok(())
    })?;
    let normal_rekey = benchmark(CRYPTO_ITERATIONS, sample.payload.len(), || {
        let envelope = rekey(
            black_box(&normal),
            key_provider,
            codec,
            &open_options,
            rotated_key_id.clone(),
        )?;
        black_box(envelope);
        Ok(())
    })?;

    println!("## {}", sample.name);
    println!();
    println!("Payload: {} bytes", sample.payload.len());
    println!();
    println!("### Disk space");
    print_size("Plain file", file_size(&plain_path)?, sample.payload.len());
    print_size("DNT normal", file_size(&normal_path)?, sample.payload.len());
    print_size(
        "DNT compact",
        file_size(&compact_path)?,
        sample.payload.len(),
    );
    println!();
    println!("### Warm-cache read path");
    print_timing("Plain file read", &plain_read);
    print_timing("DNT normal read + open", &normal_open);
    print_timing("DNT compact read + open", &compact_open);
    println!();
    println!("### Cryptographic write path");
    print_timing("DNT normal seal", &normal_seal);
    print_timing("DNT compact seal", &compact_seal);
    print_timing("DNT normal rekey", &normal_rekey);
    println!();
    Ok(())
}

fn benchmark<F>(iterations: usize, semantic_bytes: usize, mut operation: F) -> BenchResult<Stats>
where
    F: FnMut() -> BenchResult<()>,
{
    for _ in 0..WARM_UP_ITERATIONS {
        operation()?;
    }
    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        operation()?;
        samples.push(start.elapsed());
    }
    Stats::from_samples(samples, semantic_bytes)
}

struct Stats {
    mean_nanos: f64,
    median: Duration,
    p95: Duration,
    p99: Duration,
    max: Duration,
    deviation_nanos: f64,
    throughput_mib_s: f64,
}

impl Stats {
    fn from_samples(mut samples: Vec<Duration>, semantic_bytes: usize) -> BenchResult<Self> {
        if samples.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "benchmark requires at least one measured iteration",
            )
            .into());
        }
        samples.sort_unstable();
        let mean_nanos = samples
            .iter()
            .map(Duration::as_nanos)
            .map(|value| value as f64)
            .sum::<f64>()
            / samples.len() as f64;
        let variance = samples
            .iter()
            .map(Duration::as_nanos)
            .map(|value| {
                let delta = value as f64 - mean_nanos;
                delta * delta
            })
            .sum::<f64>()
            / samples.len() as f64;
        let seconds = mean_nanos / 1_000_000_000.0;
        Ok(Self {
            mean_nanos,
            median: percentile(&samples, 50),
            p95: percentile(&samples, 95),
            p99: percentile(&samples, 99),
            max: samples.last().copied().unwrap_or_default(),
            deviation_nanos: variance.sqrt(),
            throughput_mib_s: semantic_bytes as f64 / (1024.0 * 1024.0) / seconds,
        })
    }
}

fn percentile(samples: &[Duration], percentile: usize) -> Duration {
    let index = (samples.len() - 1).saturating_mul(percentile).div_ceil(100);
    samples[index]
}

fn print_environment(temp_root: &Path) {
    println!("# DNT reference comparison");
    println!();
    println!("Command: `cargo run --release -p appcore-dnt --example compare`");
    println!("Profile: release, default features");
    println!("Warm-up: {WARM_UP_ITERATIONS} operations per measured path");
    println!("Read/open samples: {READ_ITERATIONS}; seal/rekey samples: {CRYPTO_ITERATIONS}");
    println!(
        "OS/architecture: {}/{}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!(
        "Logical CPUs: {}",
        std::thread::available_parallelism().map_or(0, std::num::NonZero::get)
    );
    println!("Rust: {}", command_output("rustc", &["--version"]));
    println!("Kernel: {}", command_output("uname", &["-sr"]));
    println!("Temporary path: {}", temp_root.display());
    println!("Filesystem, storage model, power state, CPU time and peak RSS: not detected");
    println!("Read results are single-threaded warm-cache timings, not durable-I/O timings.");
    println!();
}

fn command_output(program: &str, arguments: &[&str]) -> String {
    Command::new(program)
        .args(arguments)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|output| output.trim().to_string())
        .filter(|output| !output.is_empty())
        .unwrap_or_else(|| "not detected".to_string())
}

fn print_size(label: &str, bytes: u64, plain_bytes: usize) {
    println!(
        "- {label}: {bytes} bytes ({})",
        disk_delta(bytes, plain_bytes)
    );
}

fn print_timing(label: &str, stats: &Stats) {
    println!(
        "- {label}: median {}, p95 {}, p99 {}, max {}, mean {}, deviation {}, {:.1} MiB/s semantic throughput",
        format_duration(stats.median.as_nanos() as f64),
        format_duration(stats.p95.as_nanos() as f64),
        format_duration(stats.p99.as_nanos() as f64),
        format_duration(stats.max.as_nanos() as f64),
        format_duration(stats.mean_nanos),
        format_duration(stats.deviation_nanos),
        stats.throughput_mib_s,
    );
}

struct Sample {
    name: &'static str,
    slug: &'static str,
    content_type: &'static str,
    payload: Vec<u8>,
}

impl Sample {
    fn new(
        name: &'static str,
        slug: &'static str,
        content_type: &'static str,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            name,
            slug,
            content_type,
            payload,
        }
    }
}

fn seal_options(key_id: &KeyId, content_type: &str) -> DntSealOptions {
    DntSealOptions {
        application_id: ApplicationId::new("bench.appcore").expect("static application id"),
        tenant_id: Some(TenantId::new("bench-tenant").expect("static tenant id")),
        content_type: ContentType::new(content_type).expect("static content type"),
        schema_version: 1,
        key_id: key_id.clone(),
        created_at_ms: 1_800_000_000_000,
        public_metadata: Vec::new(),
        encrypted_metadata: Vec::new(),
        flags: 0,
        max_payload_bytes: Some(MAX_PAYLOAD_BYTES),
    }
}

fn open_options(content_type: &str) -> DntOpenOptions {
    DntOpenOptions {
        application_id: ApplicationId::new("bench.appcore").expect("static application id"),
        tenant_id: Some(TenantId::new("bench-tenant").expect("static tenant id")),
        content_type: ContentType::new(content_type).expect("static content type"),
        max_payload_bytes: Some(MAX_PAYLOAD_BYTES),
    }
}

fn file_size(path: &Path) -> BenchResult<u64> {
    Ok(fs::metadata(path)?.len())
}

fn benchmark_temp_root() -> BenchResult<std::path::PathBuf> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    Ok(std::env::temp_dir().join(format!("appcore-dnt-compare-{}-{nanos}", process::id())))
}

fn json_snapshot(target_bytes: usize) -> Vec<u8> {
    let row = br#"{"kind":"snapshot","tenant":"tenant-a","stream":"runtime-state","status":"active","schema":1,"sequence":"00000123","note":"repeatable runtime record"}"#;
    let mut payload = Vec::with_capacity(target_bytes + row.len() + 2);
    payload.extend_from_slice(br#"{"records":["#);
    while payload.len() + row.len() + 2 < target_bytes {
        if !payload.ends_with(b"[") {
            payload.push(b',');
        }
        payload.extend_from_slice(row);
    }
    payload.extend_from_slice(b"]}");
    payload
}

fn deterministic_binary(size: usize) -> Vec<u8> {
    let mut state = 0x1234_5678_9abc_def0u64;
    let mut payload = Vec::with_capacity(size);
    for _ in 0..size {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        payload.push((state & 0xff) as u8);
    }
    payload
}

fn disk_delta(new_value: u64, base_value: usize) -> String {
    if new_value == base_value as u64 {
        return "plaintext baseline".to_string();
    }
    let delta = ((new_value as f64 / base_value as f64) - 1.0) * 100.0;
    if delta <= 0.0 {
        format!("{:.1}% smaller than plaintext", -delta)
    } else {
        format!("{delta:.1}% larger than plaintext")
    }
}

fn format_duration(nanos: f64) -> String {
    let micros = nanos / 1_000.0;
    if micros < 1_000.0 {
        format!("{micros:.1} us")
    } else {
        format!("{:.2} ms", micros / 1_000.0)
    }
}
