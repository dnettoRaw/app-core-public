// =============================================================================
//        #######
//     ###       ###     F: appcore-update-diagnose.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/24 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/24 00:00:00 by dnettoRaw
//      ###########      S: 1.0.3-rc
// =============================================================================

//! Read-only diagnostics for update metadata and local state.

use appcore_update::{ArtifactDescriptor, FileRecoveryStore, QuarantineStore, RecoveryDecision};
use serde::Serialize;
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_INPUT_BYTES: u64 = 1024 * 1024;
const EXIT_USAGE: i32 = 64;
const EXIT_DATA: i32 = 65;
const EXIT_NOINPUT: i32 = 66;
const EXIT_IO: i32 = 74;

fn main() {
    let result = run(env::args().skip(1).collect());
    match result {
        Ok((json_output, value, human)) => {
            if json_output {
                let rendered = serde_json::to_string(&value)
                    .unwrap_or_else(|_| "{\"schema_version\":1,\"ok\":false}".to_string());
                // appcore-norm: allow(operational-print) reason: versioned JSON is the explicit CLI stdout protocol
                println!("{rendered}");
            } else {
                // appcore-norm: allow(operational-print) reason: human diagnostics are the explicit CLI stdout protocol
                println!("{human}");
            }
        }
        Err(failure) => {
            if failure.json {
                // appcore-norm: allow(operational-print) reason: structured CLI failures are the explicit stdout protocol
                println!(
                    "{}",
                    serde_json::to_string(&failure)
                        .unwrap_or_else(|_| "{\"schema_version\":1,\"ok\":false}".to_string())
                );
            } else {
                // appcore-norm: allow(operational-print) reason: human CLI failures are the explicit stderr boundary
                eprintln!("{}", failure.message);
            }
            std::process::exit(failure.exit_code);
        }
    }
}

#[derive(Debug)]
struct Failure {
    json: bool,
    exit_code: i32,
    message: String,
}

impl Failure {
    fn new(json: bool, exit_code: i32, message: impl Into<String>) -> Self {
        Self {
            json,
            exit_code,
            message: message.into(),
        }
    }
}

impl Serialize for Failure {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serde_json::json!({
            "schema_version": 1,
            "ok": false,
            "code": "UPDATE-DIAG-ERROR",
            "exit_code": self.exit_code,
            "message": self.message,
        })
        .serialize(serializer)
    }
}

fn run(args: Vec<String>) -> Result<(bool, Value, String), Failure> {
    let json_output = args.iter().any(|arg| arg == "--json");
    let positional = args
        .into_iter()
        .filter(|arg| arg != "--json")
        .collect::<Vec<_>>();
    if positional.len() != 2 {
        return Err(Failure::new(
            json_output,
            EXIT_USAGE,
            "usage: appcore-update-diagnose [--json] <descriptor|receipt|quarantine|cache> <path>",
        ));
    }
    let kind = positional[0].as_str();
    let path = PathBuf::from(&positional[1]);
    if path.as_os_str().is_empty() {
        return Err(Failure::new(
            json_output,
            EXIT_NOINPUT,
            "diagnostic path is empty",
        ));
    }
    let value = match kind {
        "descriptor" => descriptor_report(&path, json_output)?,
        "receipt" => receipt_report(&path, json_output)?,
        "quarantine" => quarantine_report(&path, json_output)?,
        "cache" => cache_report(&path, json_output)?,
        _ => {
            return Err(Failure::new(
                json_output,
                EXIT_USAGE,
                "unknown diagnostic kind",
            ))
        }
    };
    let human = human_report(kind, &value);
    Ok((json_output, value, human))
}

fn descriptor_report(path: &Path, json_output: bool) -> Result<Value, Failure> {
    let descriptor = read_json::<ArtifactDescriptor>(path, json_output)?;
    descriptor
        .validate()
        .map_err(|_| Failure::new(json_output, EXIT_DATA, "descriptor validation failed"))?;
    Ok(json!({
        "schema_version": 1,
        "ok": true,
        "kind": "descriptor",
        "application_id": descriptor.application_id().as_str(),
        "application_version": descriptor.application_version(),
        "build_id": descriptor.build_id().as_str(),
        "channel": descriptor.channel(),
        "protocol_version": descriptor.protocol_version(),
        "sha256": descriptor.sha256(),
        "size_bytes": descriptor.size_bytes(),
        "target": descriptor.target(),
    }))
}

fn receipt_report(path: &Path, json_output: bool) -> Result<Value, Failure> {
    require_directory(path, json_output)?;
    let store = FileRecoveryStore::open(path)
        .map_err(|_| Failure::new(json_output, EXIT_IO, "receipt store is unavailable"))?;
    let decision = store
        .inspect_recovery()
        .map_err(|_| Failure::new(json_output, EXIT_DATA, "receipt is invalid or unsupported"))?;
    let phase = match &decision {
        RecoveryDecision::Idle => "idle",
        RecoveryDecision::PendingActivation { receipt } => phase_name(receipt.phase),
        RecoveryDecision::Committed { receipt } => phase_name(receipt.phase),
        RecoveryDecision::RollbackRequired { receipt } => phase_name(receipt.phase),
        RecoveryDecision::RolledBack { receipt } => phase_name(receipt.phase),
        RecoveryDecision::Aborted { receipt } => phase_name(receipt.phase),
        RecoveryDecision::ManualReviewRequired { .. } => "manual_review_required",
    };
    Ok(json!({ "schema_version": 1, "ok": true, "kind": "receipt", "phase": phase }))
}

fn quarantine_report(path: &Path, json_output: bool) -> Result<Value, Failure> {
    require_directory(path, json_output)?;
    let store = QuarantineStore::open(path, appcore_update::QUARANTINE_MAX_ENTRIES)
        .map_err(|_| Failure::new(json_output, EXIT_IO, "quarantine store is unavailable"))?;
    let entries = store.list().map_err(|_| {
        Failure::new(
            json_output,
            EXIT_DATA,
            "quarantine data is invalid or unsupported",
        )
    })?;
    Ok(json!({ "schema_version": 1, "ok": true, "kind": "quarantine", "entries": entries }))
}

fn require_directory(path: &Path, json_output: bool) -> Result<(), Failure> {
    let metadata = fs::symlink_metadata(path).map_err(|_| {
        Failure::new(
            json_output,
            EXIT_NOINPUT,
            "diagnostic directory is unavailable",
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(Failure::new(
            json_output,
            EXIT_DATA,
            "diagnostic path is not a regular directory",
        ));
    }
    Ok(())
}

fn cache_report(path: &Path, json_output: bool) -> Result<Value, Failure> {
    let mut objects = 0_u64;
    let mut parts = 0_u64;
    let mut bytes = 0_u64;
    for (name, directory, count) in [
        ("objects", path.join("objects"), &mut objects),
        ("parts", path.join("parts"), &mut parts),
    ] {
        let entries = fs::read_dir(&directory).map_err(|_| {
            Failure::new(
                json_output,
                EXIT_IO,
                format!("cache {name} directory is unavailable"),
            )
        })?;
        for entry in entries {
            let entry = entry
                .map_err(|_| Failure::new(json_output, EXIT_IO, "cache entry cannot be read"))?;
            let metadata = fs::symlink_metadata(entry.path())
                .map_err(|_| Failure::new(json_output, EXIT_IO, "cache metadata cannot be read"))?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(Failure::new(
                    json_output,
                    EXIT_DATA,
                    "cache contains a non-regular entry",
                ));
            }
            *count += 1;
            bytes = bytes.saturating_add(metadata.len());
        }
    }
    Ok(
        json!({ "schema_version": 1, "ok": true, "kind": "cache", "objects": objects, "parts": parts, "bytes": bytes }),
    )
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path, json_output: bool) -> Result<T, Failure> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| Failure::new(json_output, EXIT_NOINPUT, "input file is unavailable"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > MAX_INPUT_BYTES
    {
        return Err(Failure::new(
            json_output,
            EXIT_DATA,
            "input is not a bounded regular file",
        ));
    }
    let mut file = fs::File::open(path)
        .map_err(|_| Failure::new(json_output, EXIT_IO, "input file cannot be opened"))?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.read_to_end(&mut bytes)
        .map_err(|_| Failure::new(json_output, EXIT_IO, "input file cannot be read"))?;
    serde_json::from_slice(&bytes)
        .map_err(|_| Failure::new(json_output, EXIT_DATA, "input JSON is invalid"))
}

fn phase_name(phase: appcore_update::ActivationPhaseV2) -> &'static str {
    match phase {
        appcore_update::ActivationPhaseV2::Prepared => "prepared",
        appcore_update::ActivationPhaseV2::ActivatedPendingHealth => "activated_pending_health",
        appcore_update::ActivationPhaseV2::Committed => "committed",
        appcore_update::ActivationPhaseV2::RollbackRequired => "rollback_required",
        appcore_update::ActivationPhaseV2::RolledBack => "rolled_back",
        appcore_update::ActivationPhaseV2::Aborted => "aborted",
        appcore_update::ActivationPhaseV2::ManualReviewRequired => "manual_review_required",
    }
}

fn human_report(kind: &str, value: &Value) -> String {
    match kind {
        "descriptor" => format!(
            "descriptor: valid ({} {})",
            value["application_id"], value["application_version"]
        ),
        "receipt" => format!("receipt: phase={}", value["phase"]),
        "quarantine" => format!(
            "quarantine: {} entries",
            value["entries"].as_array().map_or(0, Vec::len)
        ),
        "cache" => format!(
            "cache: {} objects, {} parts, {} bytes",
            value["objects"], value["parts"], value["bytes"]
        ),
        _ => "diagnostic: valid".to_string(),
    }
}
