// =============================================================================
//        #######
//     ###       ###     F: diagnostics.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Redacted Runtime diagnostics assembly and durable export.

use super::status_json_value;
use crate::bootstrap::{bootstrap_runtime, now_ms, BootstrapError, BootstrapResult};
use std::fs::OpenOptions;
use std::io::Write;

fn diagnostics_json_value(app: &BootstrapResult) -> serde_json::Value {
    let _ = app.observation_file_sink.flush();
    let peer_count = app
        .peer_directory
        .lock()
        .as_ref()
        .map(|directory| directory.peers.len())
        .unwrap_or(0);
    let observations = app
        .observations
        .snapshot()
        .into_iter()
        .map(observation_json)
        .collect::<Vec<_>>();
    let (audit_journal_error, event_journal_error) = {
        let controller = app.controller.lock();
        (
            controller.instance().audit_log().durability_error(),
            controller.instance().event_bus().durability_error(),
        )
    };
    let observation_stats = app.observation_file_sink.stats();
    serde_json::json!({
        "runtime_version": env!("CARGO_PKG_VERSION"),
        "status": status_json_value(app),
        "distributed": {
            "tenant_id": app.core_identity.tenant_id.as_str(),
            "cluster_id": app.core_identity.cluster_id.as_str(),
            "core_id": app.core_identity.core_id.as_str(),
            "instance_id": app.core_identity.instance_id.as_str(),
            "protocol_version": app.core_identity.protocol_version.as_u16(),
            "operation_mode": app.operation_mode.lock().as_str(),
            "control_plane_enabled": app.config.control_plane_enabled,
            "peer_rpc_enabled": app.config.peer_rpc_enabled,
            "discovered_peers": peer_count,
            "has_leader_lease": app.leader_lease.lock().is_some()
        },
        "limits": {
            "api_max_payload_bytes": app.config.api_max_payload_bytes,
            "idempotency_ttl_ms": app.config.idempotency_ttl_ms,
            "control_plane_request_timeout_ms": app.config.control_plane_request_timeout_ms
        },
        "capabilities": app.core_manifest.capabilities,
        "audit_entries": app.controller.lock().instance().audit_log().entries().len(),
        "observations": observations,
        "operational_journal": {
            "audit_error": audit_journal_error,
            "event_error": event_journal_error
        },
        "observation_drain": {
            "written": observation_stats.written,
            "dropped": observation_stats.dropped,
            "errors": observation_stats.errors
        },
        "metrics": app.metrics.snapshot()
    })
}

fn observation_json(event: appcore_ops::ObservationEvent) -> serde_json::Value {
    serde_json::json!({
        "kind": format!("{:?}", event.kind),
        "severity": format!("{:?}", event.severity),
        "name": event.name,
        "timestamp_ms": event.timestamp_ms,
        "trace_id": event.trace_id,
        "attributes": event.attributes
    })
}

pub(super) fn run_diagnostics(
    config_path: Option<&str>,
    as_json: bool,
) -> Result<(), BootstrapError> {
    let app = bootstrap_runtime(config_path)?;
    let diagnostics = diagnostics_json_value(&app);
    if as_json {
        println!("{diagnostics}");
    } else {
        println!("runtime_version: {}", env!("CARGO_PKG_VERSION"));
        println!("app_id: {}", app.config.app_id);
        println!("core_id: {}", app.core_identity.core_id.as_str());
        println!("operation_mode: {}", app.operation_mode.lock().as_str());
        println!("capabilities: {}", app.core_manifest.capabilities.len());
        println!("observations: {}", app.observations.len());
        println!(
            "audit_entries: {}",
            app.controller.lock().instance().audit_log().entries().len()
        );
    }
    Ok(())
}

pub(super) fn run_export(
    config_path: Option<&str>,
    output: Option<&str>,
) -> Result<(), BootstrapError> {
    let output = output.ok_or_else(|| BootstrapError::Cli("missing --out".to_string()))?;
    let app = bootstrap_runtime(config_path)?;
    let audit_entries = app.controller.lock().instance().audit_log().entries();
    let export = serde_json::json!({
        "format": "appcore-runtime-diagnostics-v1",
        "generated_at_ms": now_ms(),
        "diagnostics": diagnostics_json_value(&app),
        "audit": audit_entries
    });
    let bytes = serde_json::to_vec_pretty(&export)
        .map_err(|error| BootstrapError::Runtime(error.to_string()))?;
    write_new_file(output, &bytes)?;
    println!("export created: {output}");
    Ok(())
}

fn write_new_file(path: &str, bytes: &[u8]) -> Result<(), BootstrapError> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| BootstrapError::Runtime(format!("export failed: {error}")))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| BootstrapError::Runtime(format!("export failed: {error}")))
}
