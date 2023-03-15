// =============================================================================
//        #######
//     ###       ###     F: health.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/24 11:51:10 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 13:18:47 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Stable health, activation, dependency, and runtime states.

/// Observable health of one managed service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceHealth {
    /// Startup completed and the service can accept work.
    Ready,
    /// The service is running within its declared health policy.
    Healthy,
    /// The service remains available with reduced guarantees.
    Degraded,
    /// The service cannot provide its responsibility.
    Failed,
    /// Service startup is in progress.
    Starting,
    /// Cooperative shutdown is in progress.
    Stopping,
    /// The service has not produced a trustworthy health signal.
    Unknown,
}

impl ServiceHealth {
    /// Reports whether a compatibility dependency may use this service.
    pub fn dependency_ready(self) -> bool {
        matches!(self, Self::Ready | Self::Healthy | Self::Degraded)
    }

    /// Reports whether the service is in a failed state.
    pub fn is_failed(self) -> bool {
        self == Self::Failed
    }
}

/// Installation-level activation of one managed service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceActivationState {
    /// The service is configured and participates in lifecycle management.
    Enabled,
    /// The service is intentionally disabled by deployment policy.
    Disabled,
    /// The deployment has no provider or configuration for the service.
    NotConfigured,
}

impl ServiceActivationState {
    /// Reports whether lifecycle actions may run for this service.
    pub fn is_enabled(self) -> bool {
        self == Self::Enabled
    }

    /// Reports whether deployment configuration exists for this service.
    pub fn is_configured(self) -> bool {
        self != Self::NotConfigured
    }
}

/// Concrete execution state of one managed service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceRuntimeState {
    /// Startup is in progress.
    Starting,
    /// The service currently owns its declared resource.
    Running,
    /// Cooperative stop was requested.
    StopRequested,
    /// Shutdown is in progress.
    Stopping,
    /// No service instance owns the resource.
    Stopped,
    /// A restart has been scheduled.
    RestartScheduled,
    /// A restart worker is executing lifecycle actions.
    Restarting,
    /// A previous instance outlived its shutdown timeout.
    Orphaned,
    /// Automatic lifecycle actions are disabled pending operator action.
    Quarantined,
    /// The service failed without a live orphan.
    Failed,
}

impl ServiceRuntimeState {
    /// Reports whether the service may still own an external resource.
    pub fn owns_resource(self) -> bool {
        matches!(
            self,
            Self::Starting
                | Self::Running
                | Self::StopRequested
                | Self::Stopping
                | Self::Restarting
                | Self::Orphaned
        )
    }
}

/// Minimum health accepted from a declared dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyRequirement {
    /// Accept a dependency that is ready or healthy.
    Ready,
    /// Require a steady healthy dependency.
    Healthy,
    /// Permit ready, healthy, or explicitly degraded operation.
    DegradedAllowed,
    /// Never block the dependent when this dependency is absent or unhealthy.
    Optional,
}

impl DependencyRequirement {
    /// Reports whether `health` satisfies this requirement.
    pub fn accepts(self, health: ServiceHealth) -> bool {
        match self {
            Self::Ready => matches!(health, ServiceHealth::Ready | ServiceHealth::Healthy),
            Self::Healthy => health == ServiceHealth::Healthy,
            Self::DegradedAllowed => health.dependency_ready(),
            Self::Optional => true,
        }
    }
}
