// =============================================================================
//        #######
//     ###       ###     F: service.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/24 11:51:10 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 13:18:47 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Managed-service contracts and descriptors.

use crate::{
    DependencyRequirement, RestartPolicy, ServiceActivationState, ServiceHealth,
    ServiceRuntimeState, SupervisorResult,
};
use std::time::Duration;

/// Runtime resource controlled by a managed service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedResource {
    /// Runtime lifecycle coordination.
    Runtime,
    /// Security and credential boundary.
    Security,
    /// Scheduler coordinator and task workers.
    Scheduler,
    /// Peer RPC listener and workers.
    PeerRpc,
    /// Control-plane worker.
    ControlPlane,
    /// Durable job workers and queues.
    Jobs,
    /// Update polling and activation coordination.
    Update,
    /// Auth-server listener and request workers.
    AuthServer,
    /// Metrics collection.
    Metrics,
    /// Observation drain.
    Observation,
    /// Synchronization listener or worker.
    Sync,
    /// Runtime HTTP listener.
    Http,
    /// Multi-tenant Gateway listener and connection workers.
    Gateway,
    /// Generic bounded worker.
    Worker,
    /// Generic bounded queue.
    Queue,
}

/// One dependency and the minimum health required from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceDependency {
    service_id: String,
    requirement: DependencyRequirement,
}

impl ServiceDependency {
    /// Creates a validated dependency contract.
    pub fn new(
        service_id: impl Into<String>,
        requirement: DependencyRequirement,
    ) -> SupervisorResult<Self> {
        let dependency = Self {
            service_id: service_id.into(),
            requirement,
        };
        validate_name(&dependency.service_id)?;
        Ok(dependency)
    }

    /// Returns the stable dependency service identifier.
    pub fn service_id(&self) -> &str {
        &self.service_id
    }

    /// Returns the minimum accepted dependency health.
    pub fn requirement(&self) -> DependencyRequirement {
        self.requirement
    }
}

/// Immutable definition of one managed service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceDescriptor {
    name: String,
    resource: ManagedResource,
    dependencies: Vec<ServiceDependency>,
    restart_policy: RestartPolicy,
    activation: ServiceActivationState,
    critical: bool,
}

impl ServiceDescriptor {
    /// Creates a service descriptor with no dependencies.
    pub fn new(
        name: impl Into<String>,
        resource: ManagedResource,
        restart_policy: RestartPolicy,
    ) -> SupervisorResult<Self> {
        let descriptor = Self {
            name: name.into(),
            resource,
            dependencies: Vec::new(),
            restart_policy,
            activation: ServiceActivationState::Enabled,
            critical: true,
        };
        descriptor.validate()?;
        Ok(descriptor)
    }

    /// Adds one compatibility dependency that permits degraded operation.
    pub fn with_dependency(self, dependency: impl Into<String>) -> SupervisorResult<Self> {
        self.add_dependency(ServiceDependency::new(
            dependency,
            DependencyRequirement::DegradedAllowed,
        )?)
    }

    /// Adds one dependency with an explicit minimum-health requirement.
    pub fn with_dependency_requirement(
        self,
        dependency: impl Into<String>,
        requirement: DependencyRequirement,
    ) -> SupervisorResult<Self> {
        self.add_dependency(ServiceDependency::new(dependency, requirement)?)
    }

    fn add_dependency(mut self, dependency: ServiceDependency) -> SupervisorResult<Self> {
        if dependency.service_id == self.name {
            return Err(crate::SupervisorError::InvalidConfiguration(
                "a service cannot depend on itself".to_string(),
            ));
        }
        if !self
            .dependencies
            .iter()
            .any(|current| current.service_id == dependency.service_id)
        {
            self.dependencies.push(dependency);
            self.dependencies
                .sort_by(|left, right| left.service_id.cmp(&right.service_id));
        }
        Ok(self)
    }

    /// Replaces the installation activation state.
    pub fn with_activation(mut self, activation: ServiceActivationState) -> Self {
        self.activation = activation;
        self
    }

    /// Marks whether failure affects overall Runtime health.
    pub fn with_critical(mut self, critical: bool) -> Self {
        self.critical = critical;
        self
    }

    /// Returns the stable service name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the owned Runtime resource.
    pub fn resource(&self) -> ManagedResource {
        self.resource
    }

    /// Returns required service dependencies.
    pub fn dependencies(&self) -> &[ServiceDependency] {
        &self.dependencies
    }

    /// Returns restart and shutdown policy.
    pub fn restart_policy(&self) -> RestartPolicy {
        self.restart_policy
    }

    /// Returns the installation activation state.
    pub fn activation(&self) -> ServiceActivationState {
        self.activation
    }

    /// Reports whether failure affects overall Runtime health.
    pub fn is_critical(&self) -> bool {
        self.critical
    }

    /// Validates descriptor and policy invariants.
    pub fn validate(&self) -> SupervisorResult<()> {
        validate_name(&self.name)?;
        self.restart_policy.validate()
    }
}

/// Lifecycle boundary implemented by every supervised Runtime service.
pub trait ManagedService: Send + Sync {
    /// Returns immutable identity, dependencies, resource, and policy.
    fn descriptor(&self) -> &ServiceDescriptor;
    /// Starts the service and its bounded resources.
    fn start(&self) -> SupervisorResult<()>;
    /// Requests cooperative shutdown inside `timeout`.
    fn stop(&self, timeout: Duration) -> SupervisorResult<()>;
    /// Returns current service health.
    fn health(&self) -> ServiceHealth;
    /// Returns the concrete service execution state.
    fn runtime_state(&self) -> ServiceRuntimeState {
        match self.health() {
            ServiceHealth::Starting => ServiceRuntimeState::Starting,
            ServiceHealth::Ready | ServiceHealth::Healthy | ServiceHealth::Degraded => {
                ServiceRuntimeState::Running
            }
            ServiceHealth::Stopping => ServiceRuntimeState::Stopping,
            ServiceHealth::Failed => ServiceRuntimeState::Failed,
            ServiceHealth::Unknown => ServiceRuntimeState::Stopped,
        }
    }
}

fn validate_name(name: &str) -> SupervisorResult<()> {
    if name.is_empty()
        || name.len() > 128
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(crate::SupervisorError::InvalidConfiguration(
            "service name is invalid".to_string(),
        ));
    }
    Ok(())
}
