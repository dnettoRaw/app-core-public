// =============================================================================
//        #######
//     ###       ###     F: application_host_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::application_host_contract::validate_scheduler_contract;
use crate::application_tasks::{ApplicationTaskRegistry, RegisteredApplicationTask};
use appcore_contracts::SchedulerRequirements;
use appcore_scheduler::{RetryPolicy, ScheduledTask, TaskSchedule};
use std::time::SystemTime;

fn registered_tasks() -> Vec<RegisteredApplicationTask> {
    let mut registry = ApplicationTaskRegistry::new();
    registry
        .register(
            ScheduledTask {
                id: "runtime.test".to_string(),
                schedule: TaskSchedule::Once {
                    run_at: SystemTime::now(),
                },
                retry: RetryPolicy::default(),
                priority: 1,
                trace: None,
            },
            |_| Ok(()),
        )
        .unwrap();
    registry.into_tasks()
}

#[test]
fn required_scheduler_rejects_missing_business_tasks() {
    let requirements = SchedulerRequirements::new(true, 1).unwrap();

    let error = validate_scheduler_contract(&requirements, &[]).unwrap_err();

    assert!(error.to_string().contains("registered no tasks"));
}

#[test]
fn business_tasks_require_a_scheduler_declaration() {
    let requirements = SchedulerRequirements::new(false, 0).unwrap();

    let error = validate_scheduler_contract(&requirements, &registered_tasks()).unwrap_err();

    assert!(error
        .to_string()
        .contains("absent from the application manifest"));
}

#[test]
fn declared_scheduler_accepts_registered_business_tasks() {
    let requirements = SchedulerRequirements::new(true, 1).unwrap();

    assert!(validate_scheduler_contract(&requirements, &registered_tasks()).is_ok());
}
