// =============================================================================
//        #######
//     ###       ###     F: error.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/24 11:51:10 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 13:18:47 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Controlled supervisor failures.

use std::fmt::{Display, Formatter};

/// Result returned by supervisor operations.
pub type SupervisorResult<T> = Result<T, SupervisorError>;

/// Controlled failure produced by service supervision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisorError {
    /// A service name or policy is invalid.
    InvalidConfiguration(String),
    /// A service is already registered.
    ServiceAlreadyRegistered(String),
    /// A requested service is absent.
    ServiceNotFound(String),
    /// A declared dependency is absent.
    DependencyNotFound {
        /// Dependent service.
        service: String,
        /// Missing dependency.
        dependency: String,
    },
    /// The dependency graph contains a cycle.
    DependencyCycle(Vec<String>),
    /// A required dependency is not ready.
    DependencyUnavailable {
        /// Dependent service.
        service: String,
        /// Unavailable dependency.
        dependency: String,
    },
    /// A managed service boundary failed.
    ServiceFailure {
        /// Service that failed.
        service: String,
        /// Redacted controlled reason.
        reason: String,
    },
    /// A service did not stop inside its configured deadline.
    ShutdownTimeout(String),
    /// A previous service instance still owns its resource.
    ServiceOrphaned(String),
    /// The temporal restart budget was exhausted.
    RestartBudgetExceeded(String),
    /// The bounded restart queue cannot accept more work.
    RestartQueueFull,
    /// The restart executor is stopping or unavailable.
    RestartExecutorStopped,
    /// Shared supervisor state was poisoned.
    StatePoisoned,
}

impl Display for SupervisorError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfiguration(reason) => {
                write!(formatter, "invalid supervisor configuration: {reason}")
            }
            Self::ServiceAlreadyRegistered(service) => {
                write!(formatter, "service `{service}` is already registered")
            }
            Self::ServiceNotFound(service) => {
                write!(formatter, "service `{service}` is not registered")
            }
            Self::DependencyNotFound {
                service,
                dependency,
            } => write!(
                formatter,
                "service `{service}` requires missing dependency `{dependency}`"
            ),
            Self::DependencyCycle(cycle) => {
                write!(
                    formatter,
                    "service dependency cycle: {}",
                    cycle.join(" -> ")
                )
            }
            Self::DependencyUnavailable {
                service,
                dependency,
            } => write!(
                formatter,
                "service `{service}` dependency `{dependency}` is unavailable"
            ),
            Self::ServiceFailure { service, reason } => {
                write!(formatter, "service `{service}` failed: {reason}")
            }
            Self::ShutdownTimeout(service) => {
                write!(
                    formatter,
                    "service `{service}` exceeded its shutdown timeout"
                )
            }
            Self::ServiceOrphaned(service) => {
                write!(
                    formatter,
                    "service `{service}` has an orphaned instance and cannot start"
                )
            }
            Self::RestartBudgetExceeded(service) => {
                write!(
                    formatter,
                    "service `{service}` exhausted its restart budget"
                )
            }
            Self::RestartQueueFull => formatter.write_str("restart executor queue is full"),
            Self::RestartExecutorStopped => formatter.write_str("restart executor is unavailable"),
            Self::StatePoisoned => formatter.write_str("supervisor state is poisoned"),
        }
    }
}

impl std::error::Error for SupervisorError {}
