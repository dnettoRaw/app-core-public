// =============================================================================
//        #######
//     ###       ###     F: doctor.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/24 11:51:10 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 13:18:47 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Managed-service, watchdog, executor, and external health diagnosis.

use crate::bootstrap::{bootstrap_runtime, BootstrapError};
use crate::runtime_services::{RuntimeDiagnosis, RuntimeServices};
use crate::server::RuntimeServer;
use appcore_ops::StdoutLogger;
use appcore_supervisor::{
    ServiceActivationState, ServiceHealth, ServiceSnapshot, SupervisorDiagnosis, WatchdogState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DoctorStatus {
    Healthy,
    Degraded,
    Failed,
}

impl DoctorStatus {
    fn exit_code(self) -> u8 {
        match self {
            Self::Healthy => 0,
            Self::Degraded => 1,
            Self::Failed => 2,
        }
    }
}

pub(super) fn run_doctor(config_path: Option<&str>, as_json: bool) -> Result<(), BootstrapError> {
    let app = bootstrap_runtime(config_path).map_err(invalid_configuration)?;
    let mut server = RuntimeServer::new(app, StdoutLogger::new());
    let diagnosis = RuntimeServices::diagnose(&mut server)?;
    let status = doctor_status(&diagnosis);
    if as_json {
        println!("{}", diagnosis_json(&diagnosis, status));
    } else {
        print_diagnosis(&diagnosis, status);
    }
    if status == DoctorStatus::Healthy {
        Ok(())
    } else {
        Err(BootstrapError::Exit {
            code: status.exit_code(),
            message: format!("appcore doctor: {}", status_name(status)),
        })
    }
}

fn doctor_status(diagnosis: &RuntimeDiagnosis) -> DoctorStatus {
    let supervisor = &diagnosis.supervisor;
    if !supervisor.graph_valid
        || supervisor.watchdog.state == WatchdogState::Failed
        || supervisor.watchdog.state == WatchdogState::Stalled
        || !supervisor.restart_executor.healthy
        || supervisor.services.iter().any(service_failed)
    {
        return DoctorStatus::Failed;
    }
    if supervisor.watchdog.state != WatchdogState::Healthy
        || supervisor.watchdog.reconcile_sequence < 2
        || supervisor.services.iter().any(service_degraded)
        || diagnosis
            .external_health
            .as_ref()
            .is_some_and(|health| !health.status_ok)
    {
        return DoctorStatus::Degraded;
    }
    DoctorStatus::Healthy
}

fn service_failed(service: &ServiceSnapshot) -> bool {
    service.enabled
        && (service.quarantined
            || service.operator_required
            || service.health == ServiceHealth::Failed)
}

fn service_degraded(service: &ServiceSnapshot) -> bool {
    service.enabled
        && matches!(
            service.health,
            ServiceHealth::Degraded
                | ServiceHealth::Starting
                | ServiceHealth::Stopping
                | ServiceHealth::Unknown
        )
}

fn print_diagnosis(diagnosis: &RuntimeDiagnosis, status: DoctorStatus) {
    let supervisor = &diagnosis.supervisor;
    println!("AppCore doctor: {}", status_name(status));
    println!("Supervisor");
    print_check(
        supervisor.watchdog.reconcile_sequence >= 2,
        "reconcile sequence advancing",
    );
    print_check(
        supervisor.watchdog.state == WatchdogState::Healthy,
        "watchdog healthy",
    );
    print_check(
        supervisor.restart_executor.healthy,
        "restart executor healthy",
    );
    println!("Services");
    for service in &supervisor.services {
        println!(
            "{} {} {:?}",
            service_marker(service),
            service.name,
            service_label(service)
        );
    }
    println!("External watchdog");
    match &diagnosis.external_health {
        Some(health) => {
            print_check(health.status_ok, "health endpoint valid");
            print_check(health.reconcile_sequence >= 2, "progress observed");
        }
        None => println!("[off] health endpoint disabled"),
    }
    for issue in &supervisor.issues {
        println!("[fail] {issue}");
    }
}

fn service_marker(service: &ServiceSnapshot) -> &'static str {
    if service.activation != ServiceActivationState::Enabled {
        "[off]"
    } else if service_failed(service) {
        "[fail]"
    } else if service_degraded(service) {
        "[warn]"
    } else {
        "[ok]"
    }
}

fn service_label(service: &ServiceSnapshot) -> ServiceHealth {
    service.health
}

fn print_check(ok: bool, label: &str) {
    println!("{} {label}", if ok { "[ok]" } else { "[fail]" });
}

fn diagnosis_json(diagnosis: &RuntimeDiagnosis, status: DoctorStatus) -> serde_json::Value {
    let supervisor = &diagnosis.supervisor;
    serde_json::json!({
        "status": status_name(status),
        "exit_code": status.exit_code(),
        "supervisor": supervisor_json(supervisor),
        "external_watchdog": diagnosis.external_health.as_ref().map(|health| {
            serde_json::json!({
                "health_endpoint_valid": health.status_ok,
                "progress_observed": health.reconcile_sequence >= 2,
                "reconcile_sequence": health.reconcile_sequence,
                "last_progress_at_ms": health.last_progress_at_ms
            })
        })
    })
}

fn supervisor_json(diagnosis: &SupervisorDiagnosis) -> serde_json::Value {
    serde_json::json!({
        "graph_valid": diagnosis.graph_valid,
        "issues": diagnosis.issues,
        "reconcile_sequence": diagnosis.watchdog.reconcile_sequence,
        "watchdog_state": format!("{:?}", diagnosis.watchdog.state),
        "restart_executor_healthy": diagnosis.restart_executor.healthy,
        "services": diagnosis.services.iter().map(|service| {
            serde_json::json!({
                "name": service.name,
                "health": format!("{:?}", service.health),
                "activation": format!("{:?}", service.activation),
                "runtime_state": format!("{:?}", service.runtime_state),
                "restart_state": format!("{:?}", service.restart_state),
                "quarantined": service.quarantined
            })
        }).collect::<Vec<_>>()
    })
}

fn invalid_configuration(error: BootstrapError) -> BootstrapError {
    BootstrapError::Exit {
        code: 3,
        message: format!("appcore doctor: invalid configuration: {error}"),
    }
}

fn status_name(status: DoctorStatus) -> &'static str {
    match status {
        DoctorStatus::Healthy => "healthy",
        DoctorStatus::Degraded => "degraded",
        DoctorStatus::Failed => "failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_status_exit_codes_are_stable() {
        assert_eq!(DoctorStatus::Healthy.exit_code(), 0);
        assert_eq!(DoctorStatus::Degraded.exit_code(), 1);
        assert_eq!(DoctorStatus::Failed.exit_code(), 2);
        assert_eq!(
            invalid_configuration(BootstrapError::Runtime("bad".to_string())).exit_code(),
            3
        );
    }

    #[test]
    fn doctor_classifies_supervisor_health() {
        let healthy = appcore_supervisor::Supervisor::new();
        let timestamp = crate::bootstrap::now_ms();
        healthy.reconcile(timestamp).unwrap();
        healthy.reconcile(timestamp.saturating_add(1)).unwrap();
        assert_eq!(
            doctor_status(&RuntimeDiagnosis {
                supervisor: healthy.diagnose(),
                external_health: None,
            })
            .exit_code(),
            0
        );

        let degraded = appcore_supervisor::Supervisor::new();
        assert_eq!(
            doctor_status(&RuntimeDiagnosis {
                supervisor: degraded.diagnose(),
                external_health: None,
            })
            .exit_code(),
            1
        );

        let failed = appcore_supervisor::Supervisor::new();
        failed.watchdog().mark_failed();
        assert_eq!(
            doctor_status(&RuntimeDiagnosis {
                supervisor: failed.diagnose(),
                external_health: None,
            })
            .exit_code(),
            2
        );
    }
}
