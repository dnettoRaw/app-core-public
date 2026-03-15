// =============================================================================
//        #######
//     ###       ###     F: application_supervisor_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 14:12:17 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;
use crate::supervisor::SupervisorHealthProgress;
use appcore_contracts::ProviderId;

#[cfg(unix)]
use appcore_contracts::{ApplicationId, BuildId};

#[cfg(unix)]
use appcore_update::{ArtifactDescriptor, ArtifactStore};
#[cfg(unix)]
use sha2::{Digest, Sha256};

fn update_config() -> ProviderConfig {
    ProviderConfig::new(ProviderId::new("file-update").unwrap())
        .with_setting("artifact_kind", "executable")
        .unwrap()
}

#[test]
fn executable_artifact_policy_is_explicit() {
    assert!(require_executable_artifact(&update_config()).is_ok());
    assert!(require_executable_artifact(&ProviderConfig::new(
        ProviderId::new("file-update").unwrap()
    ))
    .is_err());
}

#[test]
fn health_url_normalizes_wildcard_and_ipv6_hosts() {
    assert_eq!(
        health_url(true, "0.0.0.0", 39000).as_deref(),
        Some("http://127.0.0.1:39000/v1/health")
    );
    assert_eq!(
        health_url(true, "::1", 39000).as_deref(),
        Some("http://[::1]:39000/v1/health")
    );
    assert!(health_url(false, "127.0.0.1", 39000).is_none());
}

#[test]
fn supervisor_settings_are_bounded_and_typed() {
    let config = update_config().with_setting("max_restarts", "5").unwrap();
    assert_eq!(parse_u64_setting(&config, "max_restarts", 3).unwrap(), 5);

    let invalid = config.with_setting("max_restarts", "many").unwrap();
    assert!(parse_u64_setting(&invalid, "max_restarts", 3).is_err());
}

#[cfg(unix)]
#[test]
fn process_health_gate_commits_healthy_candidate() {
    let root = temp_root("commit");
    let store = FileArtifactStore::new(root.join("updates"));
    let bytes = b"#!/bin/sh\nmkdir -p \"$(dirname \"$APPCORE_MANAGED_HEALTH_FILE\")\"\ntouch \"$APPCORE_MANAGED_HEALTH_FILE\"\nsleep 5\n";
    let candidate = descriptor("1.1.0", "build-healthy", bytes);
    let receipt = store
        .activate(store.stage(&candidate, bytes).unwrap())
        .unwrap();
    let supervisor = test_supervisor(root.clone(), store.clone());

    let (_path, mut child) = supervisor.activate_candidate(receipt).unwrap();

    assert_eq!(store.current().unwrap(), Some(candidate));
    assert!(store.pending_activation_receipt().unwrap().is_none());
    stop_managed_child(&mut child);
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn process_health_gate_rolls_back_failed_candidate() {
    let root = temp_root("rollback");
    let store = FileArtifactStore::new(root.join("updates"));
    let previous = descriptor("1.0.0", "build-previous", b"#!/bin/sh\nsleep 5\n");
    let previous_receipt = store
        .activate(store.stage(&previous, b"#!/bin/sh\nsleep 5\n").unwrap())
        .unwrap();
    store.commit(&previous_receipt).unwrap();
    let candidate = descriptor("1.1.0", "build-failed", b"#!/bin/sh\nexit 1\n");
    let receipt = store
        .activate(store.stage(&candidate, b"#!/bin/sh\nexit 1\n").unwrap())
        .unwrap();
    let supervisor = test_supervisor(root.clone(), store.clone());

    let (_path, mut child) = supervisor.activate_candidate(receipt).unwrap();

    assert_eq!(store.current().unwrap(), Some(previous));
    assert!(store.pending_activation_receipt().unwrap().is_none());
    stop_managed_child(&mut child);
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
fn descriptor(version: &str, build: &str, bytes: &[u8]) -> ArtifactDescriptor {
    ArtifactDescriptor::new(
        ApplicationId::new("supervisor-test").unwrap(),
        version,
        BuildId::new(build).unwrap(),
        "stable",
        ">=1.0.0-rc.3",
        "1",
        format!("memory:{build}"),
        format!("{:x}", Sha256::digest(bytes)),
        bytes.len() as u64,
    )
    .unwrap()
}

#[cfg(unix)]
fn test_supervisor(root: PathBuf, store: FileArtifactStore) -> ManagedApplicationSupervisor {
    ManagedApplicationSupervisor {
        initial_executable: std::env::current_exe().unwrap(),
        application_manifest: root.join("application.toml"),
        deployment_manifest: root.join("deployment.toml"),
        update_store: store,
        health_directory: root.join("managed-health"),
        health_url: None,
        startup_timeout: Duration::from_secs(2),
        max_restarts: 1,
        health_check_interval: Duration::from_millis(10),
        watchdog_stall_timeout: Duration::from_millis(100),
        logger: StdoutLogger::new(),
    }
}

#[test]
fn external_supervisor_requests_restart_when_sequence_stops() {
    let mut tracker = ProgressTracker::default();
    let sample = |sequence| {
        Some(SupervisorHealthProgress {
            status_ok: true,
            state: "healthy".to_string(),
            reconcile_sequence: sequence,
            last_progress_at_ms: sequence,
            critical_services_healthy: true,
        })
    };

    assert_eq!(
        tracker.observe(sample(1), 100, Duration::from_millis(10)),
        ProgressState::Waiting
    );
    assert_eq!(
        tracker.observe(sample(2), 105, Duration::from_millis(10)),
        ProgressState::Advanced
    );
    let stalled = tracker.observe(sample(2), 116, Duration::from_millis(10));
    assert_eq!(stalled, ProgressState::Stalled);
    assert!(should_restart_for_progress(stalled));
    assert!(should_restart_for_progress(ProgressState::Failed));
    assert!(!should_restart_for_progress(ProgressState::Advanced));
}

#[cfg(unix)]
fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "appcore-managed-supervisor-{name}-{}-{}",
        std::process::id(),
        now_ms()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}
