// =============================================================================
//        #######
//     ###       ###     F: health_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/06/04 11:57:41 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::{BasicHealthCheck, HealthCheck, HealthReport, HealthStatus};

#[test]
fn health_report_healthy() {
    let report = HealthReport {
        status: HealthStatus::Healthy,
        message: None,
    };

    assert_eq!(report.status, HealthStatus::Healthy);
    assert_eq!(report.message, None);
}

#[test]
fn health_report_degraded_with_message() {
    let report = HealthReport {
        status: HealthStatus::Degraded,
        message: Some("latency high".to_string()),
    };

    assert_eq!(report.status, HealthStatus::Degraded);
    assert_eq!(report.message.as_deref(), Some("latency high"));
}

struct MockCheck;

impl HealthCheck for MockCheck {
    fn name(&self) -> &str {
        "mock-check"
    }

    fn check(&self) -> HealthReport {
        HealthReport {
            status: HealthStatus::Healthy,
            message: None,
        }
    }
}

#[test]
fn mock_health_check_works() {
    let check = MockCheck;
    let report = check.check();

    assert_eq!(check.name(), "mock-check");
    assert_eq!(report.status, HealthStatus::Healthy);
}

#[test]
fn basic_health_check_works() {
    let check = BasicHealthCheck::new(
        "bootstrap",
        HealthReport {
            status: HealthStatus::Healthy,
            message: None,
        },
    );
    let report = check.check();
    assert_eq!(check.name(), "bootstrap");
    assert_eq!(report.status, HealthStatus::Healthy);
}
