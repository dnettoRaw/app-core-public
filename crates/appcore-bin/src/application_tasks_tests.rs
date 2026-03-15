// =============================================================================
//        #######
//     ###       ###     F: application_tasks_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;
use appcore_scheduler::{RetryPolicy, TaskSchedule};
use std::time::SystemTime;

fn task(id: &str) -> ScheduledTask {
    ScheduledTask {
        id: id.to_string(),
        schedule: TaskSchedule::Once {
            run_at: SystemTime::now(),
        },
        retry: RetryPolicy::default(),
        priority: 1,
        trace: None,
    }
}

#[test]
fn registry_rejects_duplicate_task_ids() {
    let mut registry = ApplicationTaskRegistry::new();
    registry.register(task("runtime.test"), |_| Ok(())).unwrap();

    let error = registry
        .register(task("runtime.test"), |_| Ok(()))
        .unwrap_err();

    assert!(matches!(
        error,
        RuntimeError::RegistryItemAlreadyRegistered {
            kind: "application_task",
            ..
        }
    ));
}

#[test]
fn registry_validates_tasks_before_host_startup() {
    let mut invalid = task("runtime.test");
    invalid.schedule = TaskSchedule::Interval {
        every: std::time::Duration::ZERO,
        start_at: None,
    };

    let error = ApplicationTaskRegistry::new()
        .register(invalid, |_| Ok(()))
        .unwrap_err();

    assert!(matches!(error, RuntimeError::RegistryError(_)));
}
