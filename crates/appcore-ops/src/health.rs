// =============================================================================
//        #######
//     ###       ###     F: health.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/31 13:38:42 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Health status contracts for runtime observability checks.

/// Coarse runtime health status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    /// Component operates normally.
    Healthy,
    /// Component remains available with reduced guarantees.
    Degraded,
    /// Component allows only restricted operations.
    Restricted,
    /// Component is stopped.
    Stopped,
}

/// Result of a health check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthReport {
    /// Coarse health status.
    pub status: HealthStatus,
    /// Optional non-sensitive detail.
    pub message: Option<String>,
}

/// Contract for a component that can report health.
pub trait HealthCheck {
    /// Returns the stable check name.
    fn name(&self) -> &str;
    /// Produces a current health report.
    fn check(&self) -> HealthReport;
}

/// Basic static health check for local runtime bootstrap/status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasicHealthCheck {
    name: String,
    report: HealthReport,
}

impl BasicHealthCheck {
    /// Creates a check that returns a fixed report.
    pub fn new(name: impl Into<String>, report: HealthReport) -> Self {
        Self {
            name: name.into(),
            report,
        }
    }
}

impl HealthCheck for BasicHealthCheck {
    fn name(&self) -> &str {
        &self.name
    }

    fn check(&self) -> HealthReport {
        self.report.clone()
    }
}

#[cfg(test)]
#[path = "health_tests.rs"]
mod tests;
